use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use chrono::Utc;

pub struct StackLock {
    path: PathBuf,
}

impl StackLock {
    pub fn acquire(stack: &str) -> Result<Self, String> {
        let lock_dir = PathBuf::from("/var/lock/rehearsa");

        fs::create_dir_all(&lock_dir)
            .map_err(|e| format!("Failed to create lock dir: {}", e))?;

        let lock_path = lock_dir.join(format!("{}.lock", stack));

        // Attempt atomic lock creation (O_CREAT | O_EXCL) — eliminates TOCTOU race.
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let pid = process::id();
                let hostname = get_hostname();
                let timestamp = Utc::now().to_rfc3339();
                writeln!(file, "pid: {}", pid).ok();
                writeln!(file, "hostname: {}", hostname).ok();
                writeln!(file, "timestamp: {}", timestamp).ok();
                Ok(StackLock { path: lock_path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let content = fs::read_to_string(&lock_path).unwrap_or_default();
                let mut pid: Option<u32> = None;
                for line in content.lines() {
                    if let Some(rest) = line.strip_prefix("pid:") {
                        pid = rest.trim().parse::<u32>().ok();
                    }
                }
                match pid {
                    Some(existing_pid) if process_alive(existing_pid) => {
                        Err(format!(
                            "Stack '{}' is already being rehearsed (PID {}).",
                            stack, existing_pid
                        ))
                    }
                    _ => {
                        // Stale or corrupt lock — remove and retry once
                        let _ = fs::remove_file(&lock_path);
                        let mut file = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&lock_path)
                            .map_err(|e| format!("Failed to acquire lock after stale removal: {}", e))?;
                        let pid = process::id();
                        let hostname = get_hostname();
                        let timestamp = Utc::now().to_rfc3339();
                        writeln!(file, "pid: {}", pid).ok();
                        writeln!(file, "hostname: {}", hostname).ok();
                        writeln!(file, "timestamp: {}", timestamp).ok();
                        Ok(StackLock { path: lock_path })
                    }
                }
            }
            Err(e) => Err(format!("Failed to create lock file: {}", e)),
        }
    }
}

impl Drop for StackLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn process_alive(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{}", pid)).exists()
}

fn get_hostname() -> String {
    if let Ok(contents) = fs::read_to_string("/etc/hostname") {
        contents.trim().to_string()
    } else {
        "unknown".to_string()
    }
}
