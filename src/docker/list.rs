use bollard::Docker;
use bollard::container::ListContainersOptions;

#[allow(dead_code)]
pub async fn list_containers() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;

    let options = Some(ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    });

    let containers = docker.list_containers(options).await?;

    if containers.is_empty() {
        println!("No containers found.");
        return Ok(());
    }

    for container in containers {
        let names = container.names.unwrap_or_default();
        let image = container.image.unwrap_or_default();
        let state = container.state.unwrap_or_default();

        println!(
            "Name: {:<30} Image: {:<30} State: {}",
            names.join(","),
            image,
            state
        );
    }

    Ok(())
}
