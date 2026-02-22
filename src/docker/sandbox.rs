use bollard::Docker;
use bollard::container::{
    Config,
    CreateContainerOptions,
    StartContainerOptions,
    RemoveContainerOptions,
    InspectContainerOptions,
    LogsOptions,
};
use bollard::models::{HostConfig, ContainerStateStatusEnum};
use futures_util::stream::TryStreamExt;
use uuid::Uuid;
use tokio::time::{sleep, Duration};

pub struct SandboxResult {
    pub container_name: String,
    pub success: bool,
    pub logs: Option<String>,
    pub health: Option<String>,
}

pub async fn create_and_run_sandbox(
    image: &str,
    env: &[String],
    mounts: &[(String, String)],
    network: &str,
    timeout_seconds: u64,
) -> Result<SandboxResult, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let container_name = format!("rehearsa_sandbox_{}", Uuid::new_v4());

    let binds: Vec<String> = mounts
        .iter()
        .map(|(src, dst)| format!("{}:{}", src, dst))
        .collect();

    let env_refs: Vec<&str> = env.iter().map(|s| s.as_str()).collect();

    docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            Config {
                image: Some(image),
                env: Some(env_refs),
                host_config: Some(HostConfig {
                    binds: Some(binds),
                    network_mode: Some(network.to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await?;

    docker
        .start_container(&container_name, None::<StartContainerOptions<String>>)
        .await?;

    let mut success = false;
    let mut health_status = None;

    for _ in 0..timeout_seconds {
        let inspect = docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await?;

        if let Some(state) = inspect.state {
            if let Some(status) = state.status {
                match status {
                    ContainerStateStatusEnum::RUNNING => {
                        success = true;

                        if let Some(health) = state.health {
    if let Some(status) = health.status {
        health_status = Some(status.to_string());
    }
}
                        break;
                    }
                    ContainerStateStatusEnum::EXITED
                    | ContainerStateStatusEnum::DEAD => {
                        success = false;
                        break;
                    }
                    _ => {}
                }
            }
        }

        sleep(Duration::from_secs(1)).await;
    }

    let logs = if !success {
        let mut collected = String::new();
        let mut stream = docker.logs(
            &container_name,
            Some(LogsOptions::<String> {
                stdout: true,
                stderr: true,
                tail: "100".to_string(),
                ..Default::default()
            }),
        );

        while let Some(chunk) = stream.try_next().await? {
            collected.push_str(&chunk.to_string());
        }

        Some(collected)
    } else {
        None
    };

    Ok(SandboxResult {
        container_name,
        success,
        logs,
        health: health_status,
    })
}

pub async fn remove_sandbox(name: &str) {
    let docker = Docker::connect_with_local_defaults().unwrap();

    let _ = docker
        .remove_container(
            name,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;
}
