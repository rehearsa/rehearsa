use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use chrono::Utc;

// ======================================================
// DATA STRUCTURES
// ======================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WatchEntry {
    pub stack: String,
    pub compose_path: String,
    pub added: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WatchRegistry {
    pub watches: Vec<WatchEntry>,
}

// ======================================================
// PATH HELPERS
// ======================================================

fn rehearsa_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;
    Ok(home.join(".rehearsa"))
}

fn watches_path() -> Result<PathBuf, String> {
    // System-wide location so daemon running as root finds the same file
    Ok(PathBuf::from("/etc/rehearsa/watches.json"))
}

fn systemd_unit_path() -> PathBuf {
    PathBuf::from("/etc/systemd/system/rehearsa.service")
}

// ======================================================
// WATCH REGISTRY
// ======================================================

pub fn load_registry() -> Result<WatchRegistry, String> {
    let path = watches_path()?;
    if !path.exists() {
        return Ok(WatchRegistry::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read watches: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse watches: {}", e))
}

pub fn save_registry(registry: &WatchRegistry) -> Result<(), String> {
    // Ensure /etc/rehearsa exists
    fs::create_dir_all("/etc/rehearsa")
        .map_err(|e| format!("Failed to create /etc/rehearsa: {}", e))?;

    let path = watches_path()?;
    let json = serde_json::to_string_pretty(registry)
        .map_err(|e| format!("Failed to serialize watches: {}", e))?;
    fs::write(path, json)
        .map_err(|e| format!("Failed to write watches: {}\nTry running with sudo.", e))
}

pub fn add_watch(stack: &str, compose_path: &str) -> Result<(), String> {
    let mut registry = load_registry()?;

    // Remove existing entry for this stack if present
    registry.watches.retain(|w| w.stack != stack);

    // Resolve absolute path
    let abs_path = std::fs::canonicalize(compose_path)
        .map_err(|e| format!("Failed to resolve path '{}': {}", compose_path, e))?;

    registry.watches.push(WatchEntry {
        stack: stack.to_string(),
        compose_path: abs_path.to_string_lossy().to_string(),
        added: Utc::now().to_rfc3339(),
    });

    save_registry(&registry)?;
    println!("Watching '{}' at {}", stack, abs_path.display());
    Ok(())
}

pub fn remove_watch(stack: &str) -> Result<(), String> {
    let mut registry = load_registry()?;
    let before = registry.watches.len();
    registry.watches.retain(|w| w.stack != stack);
    if registry.watches.len() == before {
        return Err(format!("No watch found for stack '{}'", stack));
    }
    save_registry(&registry)?;
    println!("Removed watch for '{}'", stack);
    Ok(())
}

pub fn list_watches() -> Result<(), String> {
    let registry = load_registry()?;
    if registry.watches.is_empty() {
        println!("No stacks being watched.");
        println!("Add one with: rehearsa daemon watch <stack> <compose-file>");
        return Ok(());
    }

    println!("Watched Stacks");
    println!("{}", "─".repeat(60));
    println!("{:<20} {}", "Stack", "Compose Path");
    println!("{}", "─".repeat(60));
    for w in &registry.watches {
        println!("{:<20} {}", w.stack, w.compose_path);
    }
    Ok(())
}

// ======================================================
// SYSTEMD UNIT GENERATION
// ======================================================

pub fn generate_unit(user: &str, binary_path: &str) -> String {
    format!(
        r#"[Unit]
Description=Rehearsa Restore Contract Engine
Documentation=https://github.com/rehearsa/rehearsa
After=docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart={} daemon run
Restart=always
RestartSec=10
User={}
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"#,
        binary_path, user
    )
}

pub fn install_daemon() -> Result<(), String> {
    // Resolve current binary path
    let binary_path = std::env::current_exe()
        .map_err(|e| format!("Failed to resolve binary path: {}", e))?;
    let binary_str = binary_path.to_string_lossy().to_string();

    // Resolve current user (prefer SUDO_USER so we get the real user not root)
    let user = std::env::var("SUDO_USER")
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "root".to_string());

    let unit = generate_unit(&user, &binary_str);
    let unit_path = systemd_unit_path();

    // Write unit file (requires sudo)
    fs::write(&unit_path, unit)
        .map_err(|e| format!(
            "Failed to write systemd unit to {}: {}\nTry running with sudo.",
            unit_path.display(), e
        ))?;

    // Reload systemd
    run_systemctl(&["daemon-reload"])?;

    // Enable service
    run_systemctl(&["enable", "rehearsa.service"])?;

    // Start service
    run_systemctl(&["start", "rehearsa.service"])?;

    println!("Rehearsa daemon installed and started.");
    println!("Unit file: {}", unit_path.display());
    println!("Binary   : {}", binary_str);
    println!("User     : {}", user);
    println!();
    println!("Manage with:");
    println!("  systemctl status rehearsa");
    println!("  journalctl -u rehearsa -f");
    println!("  rehearsa daemon uninstall");

    Ok(())
}

pub fn uninstall_daemon() -> Result<(), String> {
    run_systemctl(&["stop", "rehearsa.service"]).ok();
    run_systemctl(&["disable", "rehearsa.service"]).ok();

    let unit_path = systemd_unit_path();
    if unit_path.exists() {
        fs::remove_file(&unit_path)
            .map_err(|e| format!("Failed to remove unit file: {}", e))?;
    }

    run_systemctl(&["daemon-reload"])?;

    println!("Rehearsa daemon uninstalled.");
    Ok(())
}

pub fn daemon_status() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["status", "rehearsa.service", "--no-pager"])
        .output()
        .map_err(|e| format!("Failed to run systemctl: {}", e))?;

    print!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(())
}

// ======================================================
// DAEMON RUN LOOP
// ======================================================

pub async fn run_daemon() -> Result<(), String> {
    use notify::{Watcher, RecursiveMode, Event};
    use notify::event::EventKind;
    use std::sync::mpsc;
    use std::time::Duration;

    let registry = load_registry()?;

    if registry.watches.is_empty() {
        eprintln!("No watches configured. Add with: rehearsa daemon watch <stack> <compose-file>");
        return Ok(());
    }

    println!("Rehearsa daemon starting...");
    println!("Watching {} stack(s):", registry.watches.len());
    for w in &registry.watches {
        println!("  {} → {}", w.stack, w.compose_path);
    }

    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();

    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }).map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Register all watched paths
    for watch in &registry.watches {
        let path = PathBuf::from(&watch.compose_path);
        if let Some(parent) = path.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch {}: {}", watch.compose_path, e))?;
        }
    }

    println!("Watching for changes. Logs via: journalctl -u rehearsa -f");

    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(Ok(event)) => {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_)
                ) {
                    for changed_path in &event.paths {
                        let registry = load_registry().unwrap_or_default();
                        for watch in &registry.watches {
                            let watch_path = PathBuf::from(&watch.compose_path);
                            if changed_path == &watch_path {
                                println!(
                                    "[{}] Change detected in {} — triggering rehearsal",
                                    Utc::now().to_rfc3339(),
                                    watch.stack
                                );
                                trigger_rehearsal(&watch.stack, &watch.compose_path).await;
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => eprintln!("Watch error: {}", e),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!("[{}] Daemon heartbeat — watching {} stacks",
                    Utc::now().to_rfc3339(),
                    load_registry().unwrap_or_default().watches.len()
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("Watcher channel disconnected — exiting");
                break;
            }
        }
    }

    Ok(())
}

// ======================================================
// REHEARSAL TRIGGER
// ======================================================

async fn trigger_rehearsal(stack: &str, compose_path: &str) {
    use crate::engine::stack::{test_stack, PullPolicy};

    println!("[{}] Starting rehearsal for '{}'", Utc::now().to_rfc3339(), stack);

    match test_stack(
        compose_path,
        120,
        false,
        None,
        false,
        PullPolicy::IfMissing,
    ).await {
        Ok(_) => println!(
            "[{}] Rehearsal complete for '{}'",
            Utc::now().to_rfc3339(), stack
        ),
        Err(e) => eprintln!(
            "[{}] Rehearsal failed for '{}': {}",
            Utc::now().to_rfc3339(), stack, e
        ),
    }
}

// ======================================================
// HELPERS
// ======================================================

fn run_systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run systemctl: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "systemctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
