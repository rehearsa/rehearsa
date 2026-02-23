use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Semaphore;
use chrono::Utc;

// ======================================================
// DAEMON CONFIG
// ======================================================

const CONFIG_PATH: &str = "/etc/rehearsa/config.json";
const DEFAULT_MAX_CONCURRENT: usize = 1;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DaemonConfig {
    /// Maximum number of rehearsals to run simultaneously.
    /// Override via REHEARSA_MAX_CONCURRENT env var or `rehearsa daemon set-concurrency`.
    #[serde(default)]
    pub max_concurrent_rehearsals: Option<usize>,
}

pub fn load_config() -> DaemonConfig {
    let raw = match fs::read_to_string(CONFIG_PATH) {
        Ok(r) => r,
        Err(_) => return DaemonConfig::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_config(config: &DaemonConfig) -> Result<(), String> {
    fs::create_dir_all("/etc/rehearsa")
        .map_err(|e| format!("Failed to create /etc/rehearsa: {}", e))?;
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(CONFIG_PATH, json)
        .map_err(|e| format!("Failed to write config: {}
Try running with sudo.", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(CONFIG_PATH, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Resolve the concurrency limit using three-tier precedence:
/// 1. REHEARSA_MAX_CONCURRENT env var
/// 2. /etc/rehearsa/config.json
/// 3. DEFAULT_MAX_CONCURRENT (1)
pub fn resolve_concurrency() -> usize {
    // Tier 1: environment variable
    if let Ok(val) = std::env::var("REHEARSA_MAX_CONCURRENT") {
        if let Ok(n) = val.trim().parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }

    // Tier 2: config file
    let config = load_config();
    if let Some(n) = config.max_concurrent_rehearsals {
        if n > 0 {
            return n;
        }
    }

    // Tier 3: default
    DEFAULT_MAX_CONCURRENT
}

pub fn set_concurrency(n: usize) -> Result<(), String> {
    if n == 0 {
        return Err("Concurrency limit must be at least 1.".to_string());
    }
    let mut config = load_config();
    config.max_concurrent_rehearsals = Some(n);
    save_config(&config)?;
    println!("Max concurrent rehearsals set to {}.", n);
    println!("Restart the daemon for the change to take effect: systemctl restart rehearsa");
    Ok(())
}

pub fn show_config() -> Result<(), String> {
    let config = load_config();
    let resolved = resolve_concurrency();

    println!();
    println!("Rehearsa Daemon Config");
    println!("{}", "─".repeat(40));
    println!(
        "  max_concurrent_rehearsals : {} (resolved: {})",
        config.max_concurrent_rehearsals
            .map(|n| n.to_string())
            .unwrap_or_else(|| "not set".to_string()),
        resolved
    );

    // Show source
    if std::env::var("REHEARSA_MAX_CONCURRENT").is_ok() {
        println!("  source: REHEARSA_MAX_CONCURRENT env var");
    } else if config.max_concurrent_rehearsals.is_some() {
        println!("  source: {}", CONFIG_PATH);
    } else {
        println!("  source: default");
    }

    println!();
    Ok(())
}

// ======================================================
// DATA STRUCTURES
// ======================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WatchEntry {
    pub stack: String,
    pub compose_path: String,
    pub added: String,
    /// Optional cron expression (5-field, e.g. "0 3 * * *"). If absent, file-watch only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// If true, run immediately on daemon start if a scheduled run was missed. Defaults false.
    #[serde(default)]
    pub catch_up: bool,
    /// Optional named backup provider to verify before each rehearsal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Optional named notify channel override for this stack.
    /// If absent, the global default channel is used (if configured).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WatchRegistry {
    pub watches: Vec<WatchEntry>,
}

// ======================================================
// PATH HELPERS
// ======================================================

#[allow(dead_code)]
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
        .map_err(|e| format!("Failed to write watches: {}\nTry running with sudo.", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(p) = watches_path() {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

pub fn add_watch(
    stack: &str,
    compose_path: &str,
    schedule: Option<&str>,
    catch_up: bool,
    provider: Option<&str>,
    notify: Option<&str>,
) -> Result<(), String> {
    use std::str::FromStr;

    let mut registry = load_registry()?;

    // Validate cron expression if provided
    if let Some(expr) = schedule {
        // Prepend seconds field (0) since the cron crate uses 6-field expressions
        let full_expr = format!("0 {}", expr);
        cron::Schedule::from_str(&full_expr)
            .map_err(|e| format!("Invalid cron expression '{}': {}", expr, e))?;
    }

    // Remove existing entry for this stack if present
    registry.watches.retain(|w| w.stack != stack);

    // Resolve absolute path
    let abs_path = std::fs::canonicalize(compose_path)
        .map_err(|e| format!("Failed to resolve path '{}': {}", compose_path, e))?;

    registry.watches.push(WatchEntry {
        stack: stack.to_string(),
        compose_path: abs_path.to_string_lossy().to_string(),
        added: Utc::now().to_rfc3339(),
        schedule: schedule.map(|s| s.to_string()),
        catch_up,
        provider: provider.map(|s| s.to_string()),
        notify: notify.map(|s| s.to_string()),
    });

    save_registry(&registry)?;
    println!("Watching '{}' at {}", stack, abs_path.display());
    if let Some(expr) = schedule {
        println!("Schedule : {}", expr);
    }
    if let Some(pname) = provider {
        println!("Provider : {}", pname);
    }
    if let Some(nchan) = notify {
        println!("Notify   : {}", nchan);
    }
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
    println!("{}", "─".repeat(110));
    println!("{:<20} {:<30} {:<16} {:<20} {}", "Stack", "Compose Path", "Schedule", "Provider", "Notify");
    println!("{}", "─".repeat(110));
    for w in &registry.watches {
        let schedule = w.schedule.as_deref().unwrap_or("—");
        let provider = w.provider.as_deref().unwrap_or("—");
        let notify   = w.notify.as_deref().unwrap_or("—");
        println!("{:<20} {:<30} {:<16} {:<20} {}", w.stack, w.compose_path, schedule, provider, notify);
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
        let sched = w.schedule.as_deref().unwrap_or("no schedule");
        let prov  = w.provider.as_deref().unwrap_or("no provider");
        println!("  {} → {}  [{}]  [{}]", w.stack, w.compose_path, sched, prov);
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

    // Shared semaphore — limits simultaneous rehearsals across scheduler and file watcher
    let max_concurrent = resolve_concurrency();
    println!("Concurrency limit: {} simultaneous rehearsal(s)", max_concurrent);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    // Spawn the cron scheduler as a separate task
    tokio::spawn(run_scheduler(Arc::clone(&semaphore)));

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
                                let sem = Arc::clone(&semaphore);
                                let stack = watch.stack.clone();
                                let compose_path = watch.compose_path.clone();
                                let provider = watch.provider.clone();
                                let notify_ch = watch.notify.clone();
                                tokio::spawn(async move {
                                    let _permit = sem.acquire().await;
                                    trigger_rehearsal(
                                        &stack,
                                        &compose_path,
                                        provider.as_deref(),
                                        notify_ch.as_deref(),
                                    ).await;
                                });
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
// CRON SCHEDULER
// ======================================================

const SCHEDULER_STATE_PATH: &str = "/etc/rehearsa/scheduler_state.json";

/// Load persisted last_run map from disk. Returns empty map if file is absent or unreadable.
fn load_scheduler_state() -> HashMap<String, chrono::DateTime<Utc>> {
    let raw = match fs::read_to_string(SCHEDULER_STATE_PATH) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Persist the last_run map to disk. Logs on failure but never panics —
/// a write failure is not worth crashing the daemon over.
fn save_scheduler_state(last_run: &HashMap<String, chrono::DateTime<Utc>>) {
    let raw = match serde_json::to_string_pretty(last_run) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Scheduler: failed to serialize state: {}", e);
            return;
        }
    };
    if let Err(e) = fs::write(SCHEDULER_STATE_PATH, raw) {
        eprintln!("Scheduler: failed to write state to {}: {}", SCHEDULER_STATE_PATH, e);
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(SCHEDULER_STATE_PATH, std::fs::Permissions::from_mode(0o600));
    }
}

/// Runs in a background task. Every 30 seconds it re-reads the registry,
/// checks whether any scheduled stack is due, and fires trigger_rehearsal.
/// Last-run times are persisted to disk so catch_up works correctly across
/// daemon restarts.
async fn run_scheduler(semaphore: Arc<Semaphore>) {
    use std::str::FromStr;
    use tokio::time::Duration;

    // Load persisted state — survives daemon restarts
    let mut last_run = load_scheduler_state();

    if !last_run.is_empty() {
        println!(
            "[{}] Scheduler: loaded last_run state for {} stack(s)",
            Utc::now().to_rfc3339(),
            last_run.len()
        );
    }

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let registry = match load_registry() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[{}] Scheduler: failed to load registry: {}", Utc::now().to_rfc3339(), e);
                continue;
            }
        };

        let now = Utc::now();

        for watch in &registry.watches {
            let expr = match &watch.schedule {
                Some(e) => e,
                None => continue, // no schedule for this stack
            };

            // cron crate requires a 6-field expression (with seconds). We store
            // 5-field (standard cron) and prepend "0 " to fix seconds at 0.
            let full_expr = format!("0 {}", expr);
            let schedule = match cron::Schedule::from_str(&full_expr) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "[{}] Scheduler: invalid cron '{}' for stack '{}': {}",
                        now.to_rfc3339(), expr, watch.stack, e
                    );
                    continue;
                }
            };

            // Find the most recent scheduled time that has already passed
            let last_fire = schedule
                .after(&(now - chrono::Duration::hours(25)))
                .take_while(|t| t <= &now)
                .last();

            let last_fire = match last_fire {
                Some(t) => t,
                None => continue, // no scheduled time has passed yet
            };

            // Have we already run this slot?
            if let Some(&prev) = last_run.get(&watch.stack) {
                if prev >= last_fire {
                    continue; // already fired this window
                }

                // A window was missed while the daemon was down.
                // If catch_up is false, record the slot and skip silently.
                if !watch.catch_up {
                    last_run.insert(watch.stack.clone(), last_fire);
                    save_scheduler_state(&last_run);
                    continue;
                }

                println!(
                    "[{}] Scheduler: catch_up triggered for '{}' (missed slot: {})",
                    now.to_rfc3339(),
                    watch.stack,
                    last_fire.to_rfc3339()
                );
            }

            // Record, persist, and fire
            last_run.insert(watch.stack.clone(), last_fire);
            save_scheduler_state(&last_run);

            println!(
                "[{}] Scheduler: running rehearsal for '{}' (schedule: {})",
                now.to_rfc3339(), watch.stack, expr
            );
            let sem = Arc::clone(&semaphore);
            let stack = watch.stack.clone();
            let compose_path = watch.compose_path.clone();
            let provider = watch.provider.clone();
            let notify_ch = watch.notify.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                println!(
                    "[{}] Semaphore acquired for '{}' — starting rehearsal",
                    chrono::Utc::now().to_rfc3339(), stack
                );
                trigger_rehearsal(
                    &stack,
                    &compose_path,
                    provider.as_deref(),
                    notify_ch.as_deref(),
                ).await;
            });
        }
    }
}

// ======================================================
// REHEARSAL TRIGGER
// ======================================================

async fn trigger_rehearsal(
    stack: &str,
    compose_path: &str,
    provider: Option<&str>,
    notify_channel: Option<&str>,
) {
    use crate::engine::stack::{test_stack, PullPolicy};
    use crate::provider::verify_provider;
    use crate::notify::{notify, NotifyEvent};

    // Provider verification — critical gate before rehearsal
    if let Some(pname) = provider {
        println!(
            "[{}] Verifying provider '{}' before rehearsal for '{}'",
            Utc::now().to_rfc3339(), pname, stack
        );
        if let Err(e) = verify_provider(pname) {
            let msg = format!("Provider '{}' verification failed: {}", pname, e);
            eprintln!("[{}] {} — skipping rehearsal for '{}'", Utc::now().to_rfc3339(), msg, stack);
            notify(stack, NotifyEvent::ProviderVerificationFailed, &msg, notify_channel);
            return;
        }
        println!(
            "[{}] Provider '{}' OK — proceeding with rehearsal for '{}'",
            Utc::now().to_rfc3339(), pname, stack
        );
    }

    println!("[{}] Starting rehearsal for '{}'", Utc::now().to_rfc3339(), stack);

    match test_stack(
        compose_path,
        120,
        false,
        None,
        false,
        PullPolicy::IfMissing,
    ).await {
        Ok(summary) => {
            println!("[{}] Rehearsal complete for '{}'", Utc::now().to_rfc3339(), stack);

            if summary.policy_violated {
                let msg = format!(
                    "Policy violation: confidence {}%, readiness {}%",
                    summary.confidence, summary.readiness
                );
                notify(stack, NotifyEvent::PolicyViolation, &msg, notify_channel);
            } else if summary.baseline_drift {
                let msg = "Restore contract drift detected against pinned baseline.".to_owned();
                notify(stack, NotifyEvent::BaselineDrift, &msg, notify_channel);
            } else {
                notify(
                    stack,
                    NotifyEvent::RehearsalRecovered,
                    "Rehearsal passed. Restore contract honoured.",
                    notify_channel,
                );
            }
        }
        Err(e) => {
            let msg = format!("{}", e);
            // Lock contention is expected when scheduler and file watcher both
            // fire simultaneously. Log as a skip, not a failure — no notification.
            if msg.contains("already being rehearsed") {
                println!(
                    "[{}] Rehearsal skipped for '{}' — already in progress (lock held)",
                    Utc::now().to_rfc3339(), stack
                );
            } else {
                let full_msg = format!("Rehearsal failed: {}", msg);
                eprintln!("[{}] {} for '{}'", Utc::now().to_rfc3339(), full_msg, stack);
                notify(stack, NotifyEvent::RehearsalFatalError, &full_msg, notify_channel);
            }
        }
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
