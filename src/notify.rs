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

// ======================================================
// EMAIL TYPES
// ======================================================

/// Which email backend to use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmailProvider {
    /// Direct SMTP delivery via lettre.
    Smtp,
    /// Sendgrid HTTP API (scaffolded — not yet implemented).
    Sendgrid,
}

impl std::fmt::Display for EmailProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailProvider::Smtp      => write!(f, "smtp"),
            EmailProvider::Sendgrid  => write!(f, "sendgrid"),
        }
    }
}

/// How the SMTP password is supplied.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmtpPasswordSource {
    /// Literal password value. Prefer smtp_password_env for production.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Name of an environment variable that holds the password.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

/// Email delivery configuration attached to a notify channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// Backend to use for delivery.
    pub provider: EmailProvider,

    // ── SMTP fields (used when provider == Smtp) ─────────────────────────

    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_host: Option<String>,

    /// Defaults to 587 (STARTTLS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub smtp_username: Option<String>,

    #[serde(default)]
    pub smtp_password: SmtpPasswordSource,

    /// Use STARTTLS (port 587 default). Set false only for local relays.
    #[serde(default = "default_true")]
    pub smtp_starttls: bool,

    // ── Sendgrid fields (used when provider == Sendgrid) ─────────────────

    /// Sendgrid API key. Prefer sendgrid_api_key_env for production.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sendgrid_api_key: Option<String>,

    /// Environment variable holding the Sendgrid API key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sendgrid_api_key_env: Option<String>,

    // ── Common fields ─────────────────────────────────────────────────────

    /// From address, e.g. "Rehearsa Alerts <alerts@example.com>"
    pub from: String,

    /// One or more recipient addresses.
    pub to: Vec<String>,
}

fn default_true() -> bool { true }

// ======================================================
// CHANNEL TYPE
// ======================================================

/// A named notify channel. Supports webhook delivery, email delivery, or both.
/// At least one transport must be configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyChannel {
    /// Unique name used to reference this channel.
    pub name: String,

    /// Webhook URL. If present, a JSON POST is sent on every event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Optional secret added as X-Rehearsa-Secret header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,

    /// Email delivery config. If present, an email is sent on every event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<EmailConfig>,
}

impl NotifyChannel {
    /// Human-readable transport summary for display.
    pub fn transport_label(&self) -> String {
        match (&self.url, &self.email) {
            (Some(_), Some(e)) => format!("webhook + email ({})", e.provider),
            (Some(_), None)    => "webhook".to_string(),
            (None, Some(e))    => format!("email ({})", e.provider),
            (None, None)       => "none".to_string(),
        }
    }
}

/// The JSON payload posted to a webhook.
#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    pub source:    &'static str,
    pub severity:  String,
    pub event:     String,
    pub stack:     String,
    pub message:   String,
    pub timestamp: String,
}

// ======================================================
// REGISTRY I/O
// ======================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NotifyRegistry {
    #[serde(default)]
    channels: HashMap<String, NotifyChannel>,

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

/// Add or update a webhook-only channel. For email channels use add_email_channel.
pub fn add_channel(name: &str, url: &str, secret: Option<&str>) -> io::Result<()> {
    let mut registry = load_registry()?;
    // Preserve existing email config if the channel already exists
    let existing_email = registry.channels.get(name).and_then(|c| c.email.clone());
    registry.channels.insert(name.to_owned(), NotifyChannel {
        name:   name.to_owned(),
        url:    Some(url.to_owned()),
        secret: secret.map(str::to_owned),
        email:  existing_email,
    });
    save_registry(&registry)?;
    println!("Notify channel '{}' registered (webhook).", name);
    Ok(())
}

/// Add or update the email transport on a channel.
pub fn add_email_channel(
    name:              &str,
    provider:          EmailProvider,
    from:              &str,
    to:                Vec<String>,
    smtp_host:         Option<&str>,
    smtp_port:         Option<u16>,
    smtp_username:     Option<&str>,
    smtp_password:     Option<&str>,
    smtp_password_env: Option<&str>,
    smtp_starttls:     bool,
    sg_api_key:        Option<&str>,
    sg_api_key_env:    Option<&str>,
) -> io::Result<()> {

    if to.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "At least one recipient address (--to) is required.",
        ));
    }

    if smtp_password.is_some() && smtp_password_env.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Specify --smtp-password or --smtp-password-env, not both.",
        ));
    }

    if sg_api_key.is_some() && sg_api_key_env.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Specify --sendgrid-api-key or --sendgrid-api-key-env, not both.",
        ));
    }

    if provider == EmailProvider::Smtp && smtp_host.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SMTP provider requires --smtp-host.",
        ));
    }

    if provider == EmailProvider::Sendgrid && sg_api_key.is_none() && sg_api_key_env.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Sendgrid provider requires --sendgrid-api-key or --sendgrid-api-key-env.",
        ));
    }

    let email_config = EmailConfig {
        provider,
        smtp_host:            smtp_host.map(str::to_owned),
        smtp_port,
        smtp_username:        smtp_username.map(str::to_owned),
        smtp_password:        SmtpPasswordSource {
            value: smtp_password.map(str::to_owned),
            env:   smtp_password_env.map(str::to_owned),
        },
        smtp_starttls,
        sendgrid_api_key:     sg_api_key.map(str::to_owned),
        sendgrid_api_key_env: sg_api_key_env.map(str::to_owned),
        from:                 from.to_owned(),
        to,
    };

    let mut registry = load_registry()?;
    let existing = registry.channels.get(name).cloned();
    let channel = NotifyChannel {
        name:   name.to_owned(),
        url:    existing.as_ref().and_then(|c| c.url.clone()),
        secret: existing.as_ref().and_then(|c| c.secret.clone()),
        email:  Some(email_config),
    };
    registry.channels.insert(name.to_owned(), channel);
    save_registry(&registry)?;
    println!("Notify channel '{}' updated with email transport.", name);
    Ok(())
}

pub fn show_channel(name: &str) -> io::Result<()> {
    let registry = load_registry()?;
    match registry.channels.get(name) {
        Some(c) => {
            let is_default = registry.default_channel.as_deref() == Some(name);
            println!("Channel   : {}{}", c.name, if is_default { "  [default]" } else { "" });
            println!("{}", "─".repeat(50));
            println!("Transport : {}", c.transport_label());
            println!();

            if let Some(ref url) = c.url {
                println!("Webhook");
                println!("  URL    : {}", url);
                println!("  Secret : {}", c.secret.as_deref().map(|_| "set").unwrap_or("not set"));
                println!();
            }

            if let Some(ref e) = c.email {
                println!("Email ({})", e.provider);
                println!("  From   : {}", e.from);
                println!("  To     : {}", e.to.join(", "));
                match e.provider {
                    EmailProvider::Smtp => {
                        println!("  Host   : {}", e.smtp_host.as_deref().unwrap_or("not set"));
                        println!("  Port   : {}", e.smtp_port.unwrap_or(587));
                        println!("  User   : {}", e.smtp_username.as_deref().unwrap_or("not set"));
                        let pw = match (&e.smtp_password.value, &e.smtp_password.env) {
                            (Some(_), _) => "set (literal)",
                            (_, Some(v)) => Box::leak(format!("env:{}", v).into_boxed_str()),
                            _            => "not set",
                        };
                        println!("  Pass   : {}", pw);
                        println!("  TLS    : {}", if e.smtp_starttls { "STARTTLS" } else { "none" });
                    }
                    EmailProvider::Sendgrid => {
                        let key = match (&e.sendgrid_api_key, &e.sendgrid_api_key_env) {
                            (Some(_), _) => "set (literal)",
                            (_, Some(v)) => Box::leak(format!("env:{}", v).into_boxed_str()),
                            _            => "not set",
                        };
                        println!("  API Key: {}", key);
                        println!("  Note   : Sendgrid delivery is scaffolded — not yet active.");
                    }
                }
            }
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
        println!("          or: rehearsa notify add-email <name> --from <addr> --to <addr> --smtp-host <host>");
        return Ok(());
    }

    println!("{:<20} {:<6} {:<24} {}", "Name", "Default", "Transport", "Destination");
    println!("{}", "─".repeat(80));
    let mut channels: Vec<&NotifyChannel> = registry.channels.values().collect();
    channels.sort_by(|a, b| a.name.cmp(&b.name));
    for c in channels {
        let is_default = registry.default_channel.as_deref() == Some(&c.name);
        let dest = match (&c.url, &c.email) {
            (Some(u), Some(e)) => format!("{}  +  {}", u, e.to.join(", ")),
            (Some(u), None)    => u.clone(),
            (None, Some(e))    => e.to.join(", "),
            (None, None)       => "—".to_string(),
        };
        println!(
            "{:<20} {:<6} {:<24} {}",
            c.name,
            if is_default { "✓" } else { "" },
            c.transport_label(),
            dest,
        );
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

    println!("Sending test notification to '{}' ({})...", name, channel.transport_label());

    let payload = WebhookPayload {
        source:    "rehearsa",
        severity:  "INFO".to_owned(),
        event:     "Test Notification".to_owned(),
        stack:     "test".to_owned(),
        message:   "This is a test notification from Rehearsa. If you received this, your channel is configured correctly.".to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let mut any_error = false;

    if channel.url.is_some() {
        match send_webhook_sync(&channel, &payload) {
            Ok(_)  => println!("  ✓ Webhook delivered."),
            Err(e) => { eprintln!("  ✗ Webhook failed: {}", e); any_error = true; }
        }
    }

    if channel.email.is_some() {
        let subject = "Rehearsa Test Notification";
        let body    = &payload.message;
        match send_email_sync(&channel, subject, body) {
            Ok(_)  => println!("  ✓ Email delivered."),
            Err(e) => { eprintln!("  ✗ Email failed: {}", e); any_error = true; }
        }
    }

    if any_error {
        std::process::exit(1);
    }

    Ok(())
}

// ======================================================
// PUBLIC LOADER (used by daemon)
// ======================================================

pub fn resolve_channel(per_stack: Option<&str>) -> Option<NotifyChannel> {
    let registry = load_registry().ok()?;
    let name = per_stack
        .or(registry.default_channel.as_deref())?;
    registry.channels.get(name).cloned()
}

// ======================================================
// DELIVERY — PUBLIC ENTRY POINT
// ======================================================

/// Fire a notification for an event across all configured transports on the
/// resolved channel. Errors are logged but never propagated — a notification
/// failure must never block or crash the daemon.
pub fn notify(stack: &str, event: NotifyEvent, message: &str, per_stack_channel: Option<&str>) {
    let channel = match resolve_channel(per_stack_channel) {
        Some(c) => c,
        None    => return,
    };

    let payload = WebhookPayload {
        source:    "rehearsa",
        severity:  event.severity().to_string(),
        event:     event.label().to_owned(),
        stack:     stack.to_owned(),
        message:   message.to_owned(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let subject = format!("[Rehearsa {}] {} — {}", payload.severity, event.label(), stack);
    let body    = format!(
        "Stack:    {}\nEvent:    {}\nSeverity: {}\nMessage:  {}\nTime:     {}",
        stack, event.label(), payload.severity, message, payload.timestamp,
    );

    // Webhook transport
    if channel.url.is_some() {
        if let Err(e) = send_webhook_sync(&channel, &payload) {
            eprintln!(
                "[{}] Notify: webhook delivery failed for '{}' on '{}': {}",
                chrono::Utc::now().to_rfc3339(), event.label(), stack, e
            );
        }
    }

    // Email transport
    if channel.email.is_some() {
        if let Err(e) = send_email_sync(&channel, &subject, &body) {
            eprintln!(
                "[{}] Notify: email delivery failed for '{}' on '{}': {}",
                chrono::Utc::now().to_rfc3339(), event.label(), stack, e
            );
        }
    }
}

// ======================================================
// DELIVERY — WEBHOOK
// ======================================================

fn send_webhook_sync(channel: &NotifyChannel, payload: &WebhookPayload) -> io::Result<()> {
    let url = match &channel.url {
        Some(u) => u,
        None    => return Ok(()),
    };

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

    cmd.arg("-d").arg(&body).arg(url);

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
// DELIVERY — EMAIL (SMTP via lettre, Sendgrid scaffolded)
// ======================================================

fn send_email_sync(channel: &NotifyChannel, subject: &str, body: &str) -> io::Result<()> {
    let cfg = match &channel.email {
        Some(e) => e,
        None    => return Ok(()),
    };

    match cfg.provider {
        EmailProvider::Smtp     => send_smtp(cfg, subject, body),
        EmailProvider::Sendgrid => send_sendgrid(cfg, subject, body),
    }
}

fn send_smtp(cfg: &EmailConfig, subject: &str, body: &str) -> io::Result<()> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{Message, SmtpTransport, Transport};
    use lettre::message::header::ContentType;

    let host = cfg.smtp_host.as_deref().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "smtp_host is required")
    })?;

    let port = cfg.smtp_port.unwrap_or(587);

    // Build recipients
    let mut message_builder = Message::builder()
        .from(cfg.from.parse().map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid from address: {}", e))
        })?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN);

    for addr in &cfg.to {
        message_builder = message_builder.to(addr.parse().map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid to address '{}': {}", addr, e))
        })?);
    }

    let email = message_builder.body(body.to_owned()).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Failed to build email: {}", e))
    })?;

    // Resolve password
    let password = match (&cfg.smtp_password.value, &cfg.smtp_password.env) {
        (Some(p), _) => Some(p.clone()),
        (_, Some(env_var)) => std::env::var(env_var).ok(),
        _ => None,
    };

    // Build transport
    let mut builder = if cfg.smtp_starttls {
        let tls_params = TlsParameters::new(host.to_owned()).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("TLS configuration error: {}", e))
        })?;
        SmtpTransport::builder_dangerous(host)
            .port(port)
            .tls(Tls::Required(tls_params))
    } else {
        SmtpTransport::builder_dangerous(host)
            .port(port)
            .tls(Tls::None)
    };

    if let (Some(username), Some(password)) = (&cfg.smtp_username, password) {
        builder = builder.credentials(Credentials::new(username.clone(), password));
    }

    let transport = builder.build();

    transport.send(&email).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("SMTP delivery failed: {}", e))
    })?;

    Ok(())
}

fn send_sendgrid(cfg: &EmailConfig, subject: &str, body: &str) -> io::Result<()> {
    // Sendgrid delivery via HTTP API — scaffolded.
    // Resolves the API key and validates config but does not yet send.
    let _api_key = match (&cfg.sendgrid_api_key, &cfg.sendgrid_api_key_env) {
        (Some(k), _) => k.clone(),
        (_, Some(env_var)) => std::env::var(env_var).map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Sendgrid API key env var '{}' not set.", env_var),
            )
        })?,
        _ => return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No Sendgrid API key configured.",
        )),
    };

    // Suppress unused variable warnings for scaffolded fields
    let _ = (subject, body, &cfg.from, &cfg.to);

    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Sendgrid delivery is scaffolded but not yet implemented. Use SMTP for now.",
    ))
}

// ======================================================
// CONSTANTS (internal)
// ======================================================

#[allow(dead_code)]
const _NOTIFY_DEFAULT_KEY: &str = NOTIFY_DEFAULT_KEY;
