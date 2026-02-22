use bollard::Docker;
use bollard::container::InspectContainerOptions;

use crate::engine::{Blueprint, Mount};

pub async fn inspect_container(name: &str) -> Result<Blueprint, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let container = docker
        .inspect_container(name, None::<InspectContainerOptions>)
        .await?;

    let mut env_vars = Vec::new();
    let mut mounts_vec = Vec::new();
    let mut image_name = String::new();
    let mut state_value = None;

    if let Some(config) = container.config {
        if let Some(image) = config.image {
            image_name = image;
        }

        if let Some(env) = config.env {
            env_vars = env;
        }
    }

    if let Some(mounts) = container.mounts {
        for mount in mounts {
            mounts_vec.push(Mount {
                source: mount.source.unwrap_or_default(),
                destination: mount.destination.unwrap_or_default(),
            });
        }
    }

    if let Some(state) = container.state {
        if let Some(status) = state.status {
            state_value = Some(status.to_string());
        }
    }

    Ok(Blueprint {
        name: name.to_string(),
        image: image_name,
        env: env_vars,
        mounts: mounts_vec,
        state: state_value,
    })
}
