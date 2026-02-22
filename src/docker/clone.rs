use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

pub fn clone_mounts(
    mounts: &[(String, String)],
) -> Result<(String, Vec<(String, String)>), Box<dyn std::error::Error>> {
    let base_dir = format!("/tmp/rehearsa_{}", Uuid::new_v4());
    fs::create_dir_all(&base_dir)?;

    let mut cloned = Vec::new();

    for (source, destination) in mounts {
        let folder_name = Path::new(destination)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let target_path = format!("{}/{}", base_dir, folder_name);

        fs::create_dir_all(&target_path)?;

        Command::new("cp")
            .args(["-a", source, &target_path])
            .status()?;

        cloned.push((target_path, destination.clone()));
    }

    Ok((base_dir, cloned))
}

pub fn cleanup_clone(base_dir: &str) {
    let _ = fs::remove_dir_all(base_dir);
}
