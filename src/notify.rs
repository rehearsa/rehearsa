use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

// ======================================================
// CONSTANTS
// ======================================================

const NOTIFY_PATH: &str = "/etc/rehearsa/notify.json";
const NOTIFY_DEFAULT_KEY: &str = "__default__";

// ======================================================
// TYPES
// ======================================================

/// Severity of a notification event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Recovery,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::Warning  => write!(f, "WARNING"),
            Severity::Recovery => write!(f, "RECOVERY"),
        }
    }
}

/// The event that triggered the notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyEvent {
    RehearsalFatalError,
    ProviderVerificationFailed,
    PolicyViolation,
    BaselineDrift,
    RehearsalRecovered,
}

impl NotifyEvent {
    pub fn severity(&self) -> Severity {
        match self {
            NotifyEvent::RehearsalFatalError         => Severity::Critical,
            NotifyEvent::ProviderVerificationFailed  => Severity::Critical,
            NotifyEvent::PolicyViolation             => Severity::Warning,
            NotifyEvent::BaselineDrift               => Severity::Warning,
            NotifyEvent::RehearsalRecovered          => Severity::Recovery,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NotifyEvent::RehearsalFatalError         => "Rehearsal Fatal Error",
            NotifyEvent::ProviderVerificationFailed  => "Provider Verification Failed",
            NotifyEvent::PolicyViolation             => "Policy Violation",
            NotifyEvent::BaselineDrift               => "Baseline Drift Detected",
            NotifyEvent::RehearsalRecovered          => "Rehearsal Recovered",
        }
    }
}

/// A named webhook channel stored in /etc/rehearsa/notify.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyChannel {
    /// Unique name used to reference this channel from watch entries or as default.
    pub name: String,

    /// Webhook URL to POST the notification payload to.
    pub url: String,

    /// Optional secret added as X-Rehearsa-Secret header for receiver validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

/// The JSON payload posted to a webhook.
#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    pub source:   &'static str,
    pub severity: String,
    pub event:    String,
    pub stack:    String,
    pub message:  String,
    pub timestamp: String,
}

// ======================================================
// REGISTRY I/O
// ======================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NotifyRegistry {
    /// Named channels keyed by name.
    #[serde(default)]
    channels: HashMap<String, NotifyChannel>,

    /// Name of the global default channel. None if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    default_channel: Option<String>,
}

fn load_registry() -> io::Result<NotifyRegistry> {
    let path = Path::new(NOTIFY_PATH);
    if !path.exists() {
        return Ok(NotifyRegistry::default());
    }
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_registry(registry: &NotifyRegistry) -> io::Result<()> {
    let path = Path::new(NOTIFY_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(registry)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, raw)?;
    Ok(())
}

// ======================================================
// PUBLIC API — CHANNEL MANAGEMENT
// ======================================================

pub fn add_channel(name: &str, url: &str, secret: Option<&str>) -> io::Result<()> {
    let mut registry = load_registry()?;
    registry.channels.insert(name.to_owned(), NotifyChannel {
        name:   name.to_owned(),
        url:    url.to_owned(),
        secret: secret.map(str::to_owned),
    });
    save_registry(&registry)?;
    println!("Notify channel '{}' registered.", name);
    Ok(())
}

pub fn show_channel(name: &str) -> io::Result<()> {
    let registry = load_registry()?;
    match registry.channels.get(name) {
        Some(c) => {
            let is_default = registry.default_channel.as_deref() == Some(name);
            println!("Channel  : {}{}", c.name, if is_default { "  [default]" } else { "" });
            println!("{}", "─".repeat(50));
            println!("URL      : {}", c.url);
            println!("Secret   : {}", c.secret.as_deref().map(|_| "set").unwrap_or("not set"));
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No notify channel found with name '{}'.", name),
            ));
        }
    }
    Ok(())
}

pub fn list_channels() -> io::Result<()> {
    let registry = load_registry()?;
    if registry.channels.is_empty() {
        println!("No notify channels registered.");
        println!("Add one with: rehearsa notify add <name> --url <webhook-url>");
        return Ok(());
    }

    println!("{:<20} {:<6} {}", "Name", "Default", "URL");
    println!("{}", "─".repeat(70));
    let mut channels: Vec<&NotifyChannel> = registry.channels.values().collect();
    channels.sort_by(|a, b| a.name.cmp(&b.name));
    for c in channels {
        let is_default = registry.default_channel.as_deref() == Some(&c.name);
        println!("{:<20} {:<6} {}", c.name, if is_default { "✓" } else { "" }, c.url);
    }
    Ok(())
}

pub fn delete_channel(name: &str) -> io::Result<()> {
    let mut registry = load_registry()?;
    if registry.channels.remove(name).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No notify channel found with name '{}'.", name),
        ));
    }
    // Clear default if it pointed to this channel
    if registry.default_channel.as_deref() == Some(name) {
        registry.default_channel = None;
        println!("Note: default channel cleared (was '{}')", name);
    }
    save_registry(&registry)?;
    println!("Notify channel '{}' deleted.", name);
    Ok(())
}

pub fn set_default(name: &str) -> io::Result<()> {
    let mut registry = load_registry()?;
    if !registry.channels.contains_key(name) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No notify channel found with name '{}'. Register it first.", name),
        ));
    }
    registry.default_channel = Some(name.to_owned());
    save_registry(&registry)?;
    println!("Default notify channel set to '{}'.", name);
    Ok(())
}

/// Send a test payload to a named channel to confirm delivery.
pub fn test_channel(name: &str) -> io::Result<()> {
    let registry = load_registry()?;
    let channel = match registry.channels.get(name) {
        Some(c) => c.clone(),
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No notify channel found with name '{}'.", name),
            ));
        }
    };

    println!("Sending test notification to '{}'...", name);

    let payload = WebhookPayload {
        source:    "rehearsa",
        severity:  "INFO".to_owned(),
        event:     "Test Notification".to_owned(),
        stack:     "test".to_owned(),
        message:   "This is a test notification from Rehearsa. If you received this, your webhook is configured correctly.".to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    match send_webhook_sync(&channel, &payload) {
        Ok(_)  => println!("✓ Test notification delivered to '{}'.", name),
        Err(e) => {
            eprintln!("✗ Failed to deliver test notification: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}

// ======================================================
// PUBLIC LOADER (used by daemon)
// ======================================================

/// Resolve the effective channel for a stack. Per-stack name takes priority
/// over the global default. Returns None if neither is configured.
pub fn resolve_channel(per_stack: Option<&str>) -> Option<NotifyChannel> {
    let registry = load_registry().ok()?;
    let name = per_stack
        .or(registry.default_channel.as_deref())?;
    registry.channels.get(name).cloned()
}

// ======================================================
// DELIVERY
// ======================================================

/// Fire a notification for an event. Resolves the channel, builds the payload,
/// and dispatches. Errors are logged but never propagated — a notification
/// failure must never block or crash the daemon.
pub fn notify(stack: &str, event: NotifyEvent, message: &str, per_stack_channel: Option<&str>) {
    let channel = match resolve_channel(per_stack_channel) {
        Some(c) => c,
        None    => return, // no channel configured — silent
    };

    let payload = WebhookPayload {
        source:    "rehearsa",
        severity:  event.severity().to_string(),
        event:     event.label().to_owned(),
        stack:     stack.to_owned(),
        message:   message.to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    if let Err(e) = send_webhook_sync(&channel, &payload) {
        eprintln!(
            "[{}] Notify: failed to deliver '{}' notification for '{}': {}",
            chrono::Utc::now().to_rfc3339(),
            event.label(),
            stack,
            e
        );
    }
}

/// Synchronous webhook POST using std + rustls-free ureq or curl fallback.
/// We use std::process::Command to shell out to curl to avoid adding a heavy
/// HTTP client dependency. Keeps the binary lean and avoids TLS crate churn.
fn send_webhook_sync(channel: &NotifyChannel, payload: &WebhookPayload) -> io::Result<()> {
    let body = serde_json::to_string(payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut cmd = std::process::Command::new("curl");
    cmd.arg("--silent")
       .arg("--show-error")
       .arg("--fail")
       .arg("--max-time").arg("10")
       .arg("-X").arg("POST")
       .arg("-H").arg("Content-Type: application/json");

    if let Some(ref secret) = channel.secret {
        cmd.arg("-H").arg(format!("X-Rehearsa-Secret: {}", secret));
    }

    cmd.arg("-d").arg(&body)
       .arg(&channel.url);

    let output = cmd.output().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to run curl (is it installed?): {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("curl error: {}", stderr.trim()),
        ));
    }

    Ok(())
}

// ======================================================
// CONSTANTS (internal)
// ======================================================

// Suppress unused warning — this key is reserved for future direct-key default storage
#[allow(dead_code)]
const _NOTIFY_DEFAULT_KEY: &str = NOTIFY_DEFAULT_KEY;
