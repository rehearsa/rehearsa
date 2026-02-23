use async_trait::async_trait;
use bollard::Docker;
use bollard::image::CreateImageOptions;
use futures_util::stream::TryStreamExt;
use std::collections::HashMap;
use std::path::Path;

use crate::docker::compose::ComposeFile;

// ======================================================
// CONTEXT
// ======================================================

pub struct PreflightContext<'a> {
    pub compose:      &'a ComposeFile,
    pub docker:       &'a Docker,
    /// Raw Compose file content — used by rules that need top-level blocks
    /// not captured in the parsed service model (e.g. networks).
    pub compose_raw:  String,
    /// Snapshot of the host environment at rehearsal time.
    /// Used by EnvVarRule to detect variables referenced in compose
    /// but absent from the restore host.
    pub environment:  HashMap<String, String>,
}

// ======================================================
// SEVERITY
// ======================================================

#[derive(Debug, Clone)]
pub enum Severity {
    /// Informational — no score penalty. Used for advisories.
    Info,
    /// Potential restore risk — small penalty.
    Warning,
    /// Likely restore failure — large penalty.
    Critical,
}

// ======================================================
// FINDING
// ======================================================

#[derive(Debug, Clone)]
pub struct PreflightFinding {
    /// Name of the rule that produced this finding.
    pub rule:     &'static str,
    pub severity: Severity,
    pub message:  String,
    pub penalty:  u32,
}

// ======================================================
// READINESS REPORT
// ======================================================

#[derive(Debug)]
pub struct RestoreReadiness {
    pub score:    u32,
    pub findings: Vec<PreflightFinding>,
}

// ======================================================
// RULE TRAIT
// ======================================================

#[async_trait]
pub trait PreflightRule: Send + Sync {
    fn name(&self) -> &'static str;

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding>;
}

// ======================================================
// RULE 1: Bind Mount Validation
// ======================================================

pub struct BindMountRule;

#[async_trait]
impl PreflightRule for BindMountRule {

    fn name(&self) -> &'static str { "BindMountRule" }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        let mut findings = Vec::new();

        for (service_name, service) in &ctx.compose.services {
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    if let Some((host_path, _container_path)) = volume.split_once(':') {

                        // Skip named volumes — only check absolute host paths
                        if !host_path.starts_with('/') {
                            continue;
                        }

                        if !Path::new(host_path).exists() {
                            findings.push(PreflightFinding {
                                rule:     self.name(),
                                severity: Severity::Critical,
                                message:  format!(
                                    "Service '{}' references missing bind path: {}",
                                    service_name, host_path
                                ),
                                penalty: 25,
                            });
                        } else {
                            // Path exists but bind mounts are still a restore risk —
                            // the data at that path may not have been restored yet.
                            findings.push(PreflightFinding {
                                rule:     self.name(),
                                severity: Severity::Info,
                                message:  format!(
                                    "Service '{}' uses bind mount '{}' — ensure data is restored before rehearsal",
                                    service_name, host_path
                                ),
                                penalty: 0,
                            });
                        }
                    }
                }
            }
        }

        findings
    }
}

// ======================================================
// RULE 2: Image Pull Validation (Fresh Host Simulation)
// ======================================================

pub struct ImagePullRule;

#[async_trait]
impl PreflightRule for ImagePullRule {

    fn name(&self) -> &'static str { "ImagePullRule" }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        let mut findings = Vec::new();

        for (service_name, service) in &ctx.compose.services {
            if let Some(image) = &service.image {

                // Warn if using :latest — non-deterministic across restore hosts
                if image.ends_with(":latest") || !image.contains(':') {
                    findings.push(PreflightFinding {
                        rule:     self.name(),
                        severity: Severity::Warning,
                        message:  format!(
                            "Service '{}' uses unpinned image tag '{}' — restore may produce a different version",
                            service_name, image
                        ),
                        penalty: 5,
                    });
                }

                // Attempt pull to simulate fresh host availability
                let options = Some(CreateImageOptions::<String> {
                    from_image: image.clone(),
                    tag: "latest".to_string(),
                    ..Default::default()
                });

                let result = ctx.docker
                    .create_image(options, None, None)
                    .try_collect::<Vec<_>>()
                    .await;

                if result.is_err() {
                    findings.push(PreflightFinding {
                        rule:     self.name(),
                        severity: Severity::Critical,
                        message:  format!(
                            "Service '{}' image '{}' cannot be pulled — restore will fail on a fresh host",
                            service_name, image
                        ),
                        penalty: 30,
                    });
                }
            }
        }

        findings
    }
}

// ======================================================
// RULE 3: Environment Variable Check
// ======================================================
//
// Docker Compose supports two env entry formats:
//   KEY=value   — explicit value, always available
//   KEY         — bare key, inherited from the host environment
//
// Bare keys are a restore risk: if the variable isn't set on the
// restore host, the container starts with it unset — silently.
// This rule surfaces all bare-key references so operators know
// exactly which variables must be present on a restore host.

pub struct EnvVarRule;

#[async_trait]
impl PreflightRule for EnvVarRule {

    fn name(&self) -> &'static str { "EnvVarRule" }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        let mut findings = Vec::new();

        for (service_name, service) in &ctx.compose.services {
            let entries = match &service.environment {
                Some(e) => e,
                None    => continue,
            };

            for entry in entries {
                // Bare key — no '=' present — means "inherit from host"
                if !entry.contains('=') {
                    let key = entry.trim();

                    if ctx.environment.contains_key(key) {
                        // Present on this host — advisory only, still a risk
                        // on a different restore host
                        findings.push(PreflightFinding {
                            rule:     self.name(),
                            severity: Severity::Info,
                            message:  format!(
                                "Service '{}' inherits '{}' from host environment — \
                                 must be set on any restore host",
                                service_name, key
                            ),
                            penalty: 0,
                        });
                    } else {
                        // Not set on this host — container will start with
                        // this variable unset, which is likely a misconfiguration
                        findings.push(PreflightFinding {
                            rule:     self.name(),
                            severity: Severity::Critical,
                            message:  format!(
                                "Service '{}' requires env var '{}' but it is not set \
                                 on this host — container will start misconfigured",
                                service_name, key
                            ),
                            penalty: 20,
                        });
                    }
                }
                // KEY=value entries are self-contained — no finding needed
            }
        }

        findings
    }
}

// ======================================================
// RULE 4: External Network Validation
// ======================================================
//
// Stacks that declare external networks require those networks
// to exist on the restore host before the stack can start.
// This rule detects missing external networks and flags them
// as Critical — the stack will not start without them.

pub struct ExternalNetworkRule;

#[async_trait]
impl PreflightRule for ExternalNetworkRule {

    fn name(&self) -> &'static str { "ExternalNetworkRule" }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        use crate::docker::compose::extract_external_networks;
        use bollard::network::ListNetworksOptions;
        use std::collections::HashMap as HM;

        let mut findings = Vec::new();

        let external = extract_external_networks(&ctx.compose_raw);
        if external.is_empty() {
            return findings;
        }

        // Fetch existing Docker networks
        let opts: ListNetworksOptions<String> = ListNetworksOptions {
            filters: HM::new(),
        };

        let existing: std::collections::HashSet<String> = match ctx.docker.list_networks(Some(opts)).await {
            Ok(networks) => networks
                .into_iter()
                .filter_map(|n| n.name)
                .collect(),
            Err(_) => {
                // Cannot reach Docker — skip rule rather than false-flag
                return findings;
            }
        };

        for network in external {
            if !existing.contains(&network) {
                findings.push(PreflightFinding {
                    rule:     self.name(),
                    severity: Severity::Critical,
                    message:  format!(
                        "External network '{}' does not exist on this host — stack will fail to start on restore",
                        network
                    ),
                    penalty: 25,
                });
            } else {
                findings.push(PreflightFinding {
                    rule:     self.name(),
                    severity: Severity::Info,
                    message:  format!(
                        "External network '{}' exists — must also be created before restore on any other host",
                        network
                    ),
                    penalty: 0,
                });
            }
        }

        findings
    }
}

// ======================================================
// RULE ENGINE
// ======================================================

pub async fn run_preflight(
    ctx: &PreflightContext<'_>,
) -> RestoreReadiness {

    let rules: Vec<Box<dyn PreflightRule>> = vec![
        Box::new(BindMountRule),
        Box::new(ImagePullRule),
        Box::new(EnvVarRule),
        Box::new(ExternalNetworkRule),
    ];

    let mut findings = Vec::new();

    for rule in rules {
        let mut results = rule.evaluate(ctx).await;
        findings.append(&mut results);
    }

    let score = compute_score(&findings);

    RestoreReadiness { score, findings }
}

// ======================================================
// SCORE COMPUTATION
// ======================================================

fn compute_score(findings: &[PreflightFinding]) -> u32 {
    let mut score: u32 = 100;
    for finding in findings {
        score = score.saturating_sub(finding.penalty);
    }
    score
}
