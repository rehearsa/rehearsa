use anyhow::{Result, anyhow};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, StartContainerOptions, NetworkingConfig,
};
use bollard::network::CreateNetworkOptions;
use bollard::image::CreateImageOptions;
use bollard::models::{
    HostConfig, Mount, EndpointSettings,
    ContainerStateStatusEnum, HealthStatusEnum, HealthConfig,
};
use futures_util::stream::TryStreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use std::time::Instant;

use crate::docker::compose::HealthCheck;
use crate::engine::graph::topological_sort;
use crate::engine::preflight::{PreflightContext, run_preflight, Severity};
use crate::lock::StackLock;
use crate::history::{
    RunRecord,
    persist,
    now_timestamp,
    validate_stack_integrity,
    calculate_stability,
    analyze_regression,
};
use crate::policy::load_policy;
use crate::baseline::{load_baseline, compare_to_baseline};
// ======================================================
// PULL POLICY
// ======================================================

#[derive(Clone)]
pub enum PullPolicy {
    Always,
    IfMissing,
    Never,
}

// ======================================================
// RUN SUMMARY (NEW)
// ======================================================

#[derive(Debug, Clone)]
pub struct StackRunSummary {
    pub stack: String,
    pub readiness: u32,
    pub confidence: u32,
    pub duration: u64,
    pub risk: String,
    pub service_scores: HashMap<String, u32>,
    pub policy_violated: bool,
    pub baseline_drift: bool,
}

// ======================================================
// STACK TEST
// ======================================================

pub async fn test_stack(
    path: &str,
    timeout: u64,
    json_output: bool,
    inject_failure: Option<String>,
    strict_integrity: bool,
    pull_policy: PullPolicy,
) -> Result<StackRunSummary> {

    let compose_path = Path::new(path);
    let stack_name = compose_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if strict_integrity {
        validate_stack_integrity(&stack_name)
            .map_err(|e| anyhow!(e))?;
    }

    let docker = Docker::connect_with_local_defaults()?;
    let _lock = StackLock::acquire(&stack_name)
        .map_err(|e| anyhow!(e))?;

    let start_time = Instant::now();

    let content = fs::read_to_string(path)?;
    let compose = crate::docker::compose::parse_compose(&content).map_err(|e| anyhow!(e))?;

    // ======================================================
    // PREFLIGHT
    // ======================================================

    let env_map: HashMap<String, String> = std::env::vars().collect();

    let preflight_ctx = PreflightContext {
        compose: &compose,
        docker: &docker,
        environment: env_map,
    };

    let readiness = run_preflight(&preflight_ctx).await;

    if !json_output {
        println!();
        println!("Preflight: Fresh Host Readiness");
        println!("--------------------------------");

        for finding in &readiness.findings {
            match finding.severity {
                Severity::Critical => println!("❌ [{}] {}", finding.rule, finding.message),
                Severity::Warning  => println!("⚠  [{}] {}", finding.rule, finding.message),
                Severity::Info     => println!("ℹ  [{}] {}", finding.rule, finding.message),
            }
        }

        println!("Restore Readiness Score: {}%", readiness.score);
        println!();
    }

    if !json_output {
        println!(
            "Starting restore simulation for '{}' ({} services)...",
            stack_name,
            compose.services.len()
        );
    }

    let run_id = Uuid::new_v4().to_string();
    let network_name = format!("rehearsa_stack_{}", run_id);

    let mut created_containers = Vec::new();
    let mut service_scores: HashMap<String, u32> = HashMap::new();

    let execution = async {

        let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();

        for (name, service) in &compose.services {
            dep_map.insert(
                name.clone(),
                service.depends_on.clone().unwrap_or_default(),
            );
        }

        let order = topological_sort(&dep_map)
            .map_err(|e| anyhow!(e))?;

        docker.create_network(CreateNetworkOptions {
            name: network_name.clone(),
            check_duplicate: true,
            driver: "bridge".to_string(),
            ..Default::default()
        }).await?;

        for service_name in order {

            let service = compose.services
                .get(&service_name)
                .ok_or_else(|| anyhow!("Missing service {}", service_name))?;

            let image = service.image.clone()
                .ok_or_else(|| anyhow!("Service {} has no image", service_name))?;

            match pull_policy {
                PullPolicy::Always => pull_image(&docker, &image).await?,
                PullPolicy::IfMissing => {
                    if docker.inspect_image(&image).await.is_err() {
                        pull_image(&docker, &image).await?;
                    }
                }
                PullPolicy::Never => {
                    if docker.inspect_image(&image).await.is_err() {
                        return Err(anyhow!(
                            "Image '{}' not present and pull policy = Never",
                            image
                        ));
                    }
                }
            }

            let container_name =
                format!("rehearsa_{}_{}", run_id, service_name);

            let mut endpoints: HashMap<String, EndpointSettings> = HashMap::new();

            endpoints.insert(
                network_name.clone(),
                EndpointSettings {
                    aliases: Some(vec![service_name.clone()]),
                    ..Default::default()
                },
            );

            let health_config = service.healthcheck
                .as_ref()
                .map(convert_healthcheck);

            let config = Config {
                image: Some(image),
                env: service.environment.clone(),
                cmd: service.command.clone(),
                healthcheck: health_config,
                host_config: Some(HostConfig {
                    mounts: Some(Vec::<Mount>::new()),
                    ..Default::default()
                }),
                networking_config: Some(NetworkingConfig {
                    endpoints_config: endpoints,
                }),
                ..Default::default()
            };

            docker.create_container(
                Some(CreateContainerOptions {
                    name: container_name.clone(),
                    platform: None,
                }),
                config,
            ).await?;

            docker.start_container(
                &container_name,
                None::<StartContainerOptions<String>>,
            ).await?;

            created_containers.push(container_name.clone());

            let mut score =
                wait_and_score(&docker, &container_name, timeout).await?;

            if let Some(ref target) = inject_failure {
                if target == &service_name {
                    score = 0;
                }
            }

            service_scores.insert(service_name.clone(), score);
        }

        Ok::<(), anyhow::Error>(())
    }.await;

    for container in &created_containers {
        let _ = docker.remove_container(
            container,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        ).await;
    }

    let _ = docker.remove_network(&network_name).await;

    if let Err(e) = execution {
    return Err(e);
}
    // ======================================================
    // SCORING
    // ======================================================

    let total: u32 = service_scores.values().sum();
    let confidence = total / service_scores.len() as u32;

    let risk = match confidence {
        90..=100 => "LOW",
        70..=89 => "MODERATE",
        40..=69 => "HIGH",
        _ => "CRITICAL",
    };

    let duration = start_time.elapsed().as_secs();

    let regression = analyze_regression(
        &stack_name,
        confidence,
        Some(readiness.score),
        duration,
    );

    let stability = calculate_stability(&stack_name, 5);
// ======================================================
// BASELINE DRIFT DETECTION
// ======================================================

let mut baseline_drift_detected = false;

if let Some(baseline) = load_baseline(&stack_name) {

    let drift = compare_to_baseline(
        &baseline,
        &service_scores,
        confidence,
        Some(readiness.score),
        duration,
    );

    let has_drift =
        !drift.new_services.is_empty()
        || !drift.missing_services.is_empty()
        || drift.confidence_delta != 0
        || drift.readiness_delta.unwrap_or(0) != 0
        || drift.duration_delta_percent.unwrap_or(0) != 0;

    if has_drift {
        baseline_drift_detected = true;
    }

    if has_drift && !json_output {

        println!();
        println!("BASELINE DRIFT DETECTED");
        println!("-----------------------");

        for svc in drift.new_services {
            println!("+ New service: {}", svc);
        }

        for svc in drift.missing_services {
            println!("- Missing service: {}", svc);
        }

        if drift.confidence_delta != 0 {
            println!("Confidence delta: {}%", drift.confidence_delta);
        }

        if let Some(r) = drift.readiness_delta {
            if r != 0 {
                println!("Readiness delta: {}%", r);
            }
        }

        if let Some(d) = drift.duration_delta_percent {
            if d != 0 {
                println!("Duration delta: {}%", d);
            }
        }

        println!();
    }
}
// ======================================================
// POLICY ENFORCEMENT
// ======================================================

let mut policy_violation = false;

if let Some(policy) = load_policy(&stack_name) {

    if let Some(min) = policy.min_confidence {
        if confidence < min {
            eprintln!(
                "POLICY VIOLATION: confidence {} below minimum {}",
                confidence, min
            );
            policy_violation = true;
        }
    }

    if let Some(min_ready) = policy.min_readiness {
        if readiness.score < min_ready {
            eprintln!(
                "POLICY VIOLATION: restore readiness {} below minimum {}",
                readiness.score, min_ready
            );
            policy_violation = true;
        }
    }

    if policy.block_on_regression.unwrap_or(false) {
        if let Some(delta) = regression.confidence_delta {
            if delta < 0 {
                eprintln!("POLICY VIOLATION: regression detected");
                policy_violation = true;
            }
        }
    }

    if policy.fail_on_new_service_failure.unwrap_or(false) {
        for (service, score) in &service_scores {
            if *score == 0 {
                eprintln!(
                    "POLICY VIOLATION: service '{}' failed to boot",
                    service
                );
                policy_violation = true;
            }
        }
    }

    if policy.fail_on_duration_spike.unwrap_or(false) {
        if let Some(spike) = regression.duration_delta_percent {
            if spike > policy.duration_spike_percent.unwrap_or(50) as i32 {
                eprintln!(
                    "POLICY VIOLATION: duration spike {}%",
                    spike
                );
                policy_violation = true;
            }
        }
    }

    // --------------------------------------------------
    // Baseline Drift Enforcement (NEW)
    // --------------------------------------------------
    if policy.fail_on_baseline_drift.unwrap_or(false) {
        if baseline_drift_detected {
            eprintln!("POLICY VIOLATION: baseline drift detected");
            policy_violation = true;
        }
    }
}

// ======================================================
// JSON OUTPUT
// ======================================================

if json_output {
    println!("{}", serde_json::to_string_pretty(&json!({
        "stack": stack_name,
        "restore_readiness": readiness.score,
        "confidence": confidence,

        "previous_confidence": regression.previous_confidence,
        "confidence_delta": regression.confidence_delta,
        "confidence_trend": regression.confidence_trend,

        "previous_readiness": regression.previous_readiness,
        "readiness_delta": regression.readiness_delta,
        "readiness_trend": regression.readiness_trend,

        "duration_delta_percent": regression.duration_delta_percent,

        "baseline_drift_detected": baseline_drift_detected,

        "stability": stability,
        "risk": risk,
        "services": service_scores
    }))?);
}

// ======================================================
// EXIT LOGIC
// ======================================================

let exit_code =
    if policy_violation {
        4
    } else if baseline_drift_detected
        && load_policy(&stack_name)
            .and_then(|p| p.fail_on_baseline_drift)
            .unwrap_or(false)
    {
        5
    } else if confidence >= 70 {
        0
    } else if confidence >= 40 {
        2
    } else {
        3
    };

// Create summary BEFORE moving values
let summary = StackRunSummary {
    stack: stack_name.clone(),
    readiness: readiness.score,
    confidence,
    duration,
    risk: risk.to_string(),
    service_scores: service_scores.clone(),
    policy_violated: policy_violation,
    baseline_drift: baseline_drift_detected,
};

let record = RunRecord {
    stack: stack_name,
    timestamp: now_timestamp(),
    duration_seconds: duration,
    confidence,
    readiness: Some(readiness.score),
    risk: risk.to_string(),
    exit_code,
    services: service_scores,
    hash: None,
};

let _ = persist(&record);

Ok(summary)
}
// ======================================================
// IMAGE PULL
// ======================================================

async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
    let options = Some(CreateImageOptions {
        from_image: image,
        ..Default::default()
    });

    docker
        .create_image(options, None, None)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}

// ======================================================
// HEALTHCHECK
// ======================================================

fn convert_healthcheck(h: &HealthCheck) -> HealthConfig {
    HealthConfig {
        test: h.test.clone(),
        interval: parse_duration(&h.interval),
        timeout: parse_duration(&h.timeout),
        retries: h.retries.map(|r| r as i64),
        start_period: None,
        start_interval: None,
    }
}

fn parse_duration(input: &Option<String>) -> Option<i64> {
    if let Some(val) = input {
        if let Some(stripped) = val.strip_suffix("s") {
            if let Ok(secs) = stripped.parse::<i64>() {
                return Some(secs * 1_000_000_000);
            }
        }
    }
    None
}

// ======================================================
// WAIT + SCORE
// ======================================================

async fn wait_and_score(
    docker: &Docker,
    container: &str,
    timeout: u64,
) -> Result<u32> {

    let mut elapsed = 0;

    while elapsed < timeout {

        let inspect = docker.inspect_container(container, None).await?;

        if let Some(state) = inspect.state {
            match state.status {

                Some(ContainerStateStatusEnum::RUNNING) => {
                    if let Some(health) = state.health {
                        match health.status {
                            Some(HealthStatusEnum::HEALTHY) => return Ok(100),
                            Some(HealthStatusEnum::UNHEALTHY) => return Ok(40),
                            _ => {}
                        }
                    } else {
                        return Ok(85);
                    }
                }

                Some(ContainerStateStatusEnum::EXITED) => return Ok(0),

                _ => {}
            }
        }

        sleep(Duration::from_secs(1)).await;
        elapsed += 1;
    }

    Ok(0)
}
