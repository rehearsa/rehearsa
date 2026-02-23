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
    pub compose: &'a ComposeFile,
    pub docker: &'a Docker,
    pub environment: HashMap<String, String>,
}

// ======================================================
// SEVERITY
// ======================================================

#[derive(Debug, Clone)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

// ======================================================
// FINDING
// ======================================================

#[derive(Debug, Clone)]
pub struct PreflightFinding {
    pub rule: &'static str,
    pub severity: Severity,
    pub message: String,
    pub penalty: u32,
}

// ======================================================
// READINESS REPORT
// ======================================================

#[derive(Debug)]
pub struct RestoreReadiness {
    pub score: u32,
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

    fn name(&self) -> &'static str {
        "BindMountRule"
    }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        let mut findings = Vec::new();

        for (service_name, service) in &ctx.compose.services {

            if let Some(volumes) = &service.volumes {

                for volume in volumes {

                    if let Some((host_path, _container_path)) = volume.split_once(':') {

                        // Skip named volumes
                        if !host_path.starts_with('/') {
                            continue;
                        }

                        if !Path::new(host_path).exists() {
                            findings.push(PreflightFinding {
                                rule: self.name(),
                                severity: Severity::Critical,
                                message: format!(
                                    "Service '{}' references missing bind path: {}",
                                    service_name,
                                    host_path
                                ),
                                penalty: 25,
                            });
                        } else {
                            findings.push(PreflightFinding {
                                rule: self.name(),
                                severity: Severity::Warning,
                                message: format!(
                                    "Service '{}' uses bind mount: {}",
                                    service_name,
                                    host_path
                                ),
                                penalty: 5,
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

    fn name(&self) -> &'static str {
        "ImagePullRule"
    }

    async fn evaluate(
        &self,
        ctx: &PreflightContext<'_>,
    ) -> Vec<PreflightFinding> {

        let mut findings = Vec::new();

        for (service_name, service) in &ctx.compose.services {

            if let Some(image) = &service.image {

                // Warn if using :latest
                if image.ends_with(":latest") {
                    findings.push(PreflightFinding {
                        rule: self.name(),
                        severity: Severity::Warning,
                        message: format!(
                            "Service '{}' uses :latest tag (non-deterministic restore)",
                            service_name
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
                        rule: self.name(),
                        severity: Severity::Critical,
                        message: format!(
                            "Service '{}' image '{}' cannot be pulled",
                            service_name,
                            image
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
// RULE ENGINE
// ======================================================

pub async fn run_preflight(
    ctx: &PreflightContext<'_>,
) -> RestoreReadiness {

    let rules: Vec<Box<dyn PreflightRule>> = vec![
        Box::new(BindMountRule),
        Box::new(ImagePullRule),
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
