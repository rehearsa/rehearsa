use bollard::Docker;
use bollard::network::CreateNetworkOptions;
use uuid::Uuid;

pub async fn create_isolated_network() -> Result<String, Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let network_name = format!("rehearsa_net_{}", Uuid::new_v4());

    docker
        .create_network(CreateNetworkOptions {
            name: network_name.clone(),
            check_duplicate: true,
            driver: "bridge".to_string(),
            internal: true,
            attachable: false,
            ingress: false,
            ipam: Default::default(),
            enable_ipv6: false,
            options: Default::default(),
            labels: Default::default(),
        })
        .await?;

    Ok(network_name)
}

pub async fn remove_network(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    docker.remove_network(name).await?;

    Ok(())
}
