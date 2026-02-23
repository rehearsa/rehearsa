use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

// ======================================================
// CONSTANTS
// ======================================================

const PROVIDERS_PATH: &str = "/etc/rehearsa/providers.json";

// ======================================================
// TYPES
// ======================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Restic,
    Borg,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderKind::Restic => write!(f, "restic"),
            ProviderKind::Borg   => write!(f, "borg"),
        }
    }
}

/// Credential source for the backup repository password.
/// Exactly one of these should be set.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PasswordSource {
    /// Name of an environment variable that holds the password.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,

    /// Path to a file containing the password.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// Model B scaffold: verification options applied before a rehearsal runs.
/// All fields are optional — absent means "no verification required".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerifyOptions {
    /// Maximum age (in hours) of the most recent snapshot before verification fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_snapshot_age_hours: Option<u64>,

    /// If true, attempt a dry-run restore from the latest snapshot before rehearsal.
    #[serde(default)]
    pub test_restore: bool,
}

/// A named backup provider definition stored in /etc/rehearsa/providers.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Unique name used to reference this provider from watch entries.
    pub name: String,

    /// The backup tool this provider represents.
    pub kind: ProviderKind,

    /// Repository path or URI (e.g. /mnt/backups/restic or s3:bucket/path).
    pub repository: String,

    /// How the repository password is supplied.
    #[serde(default)]
    pub password: PasswordSource,

    /// Model B scaffold: verification behaviour.
    #[serde(default)]
    pub verify: VerifyOptions,
}

// ======================================================
// REGISTRY I/O
// ======================================================

fn load_registry() -> io::Result<HashMap<String, ProviderConfig>> {
    let path = Path::new(PROVIDERS_PATH);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path)?;
    let map: HashMap<String, ProviderConfig> =
        serde_json::from_str(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(map)
}

fn save_registry(registry: &HashMap<String, ProviderConfig>) -> io::Result<()> {
    let path = Path::new(PROVIDERS_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(registry)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, raw)?;
    Ok(())
}

// ======================================================
// PUBLIC API
// ======================================================

/// Register or update a provider. Overwrites any existing entry with the same name.
pub fn add_provider(
    name: &str,
    kind_str: &str,
    repository: &str,
    password_env: Option<&str>,
    password_file: Option<&str>,
) -> io::Result<()> {
    let kind = match kind_str {
        "restic" => ProviderKind::Restic,
        "borg"   => ProviderKind::Borg,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unknown provider kind '{}'. Supported: restic, borg", other),
            ))
        }
    };

    if password_env.is_some() && password_file.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Specify --password-env or --password-file, not both",
        ));
    }

    let password = PasswordSource {
        env: password_env.map(str::to_owned),
        file: password_file.map(str::to_owned),
    };

    let config = ProviderConfig {
        name: name.to_owned(),
        kind,
        repository: repository.to_owned(),
        password,
        verify: VerifyOptions::default(),
    };

    let mut registry = load_registry()?;
    registry.insert(name.to_owned(), config);
    save_registry(&registry)?;

    println!("Provider '{}' registered.", name);
    Ok(())
}

/// Print a single provider's config.
pub fn show_provider(name: &str) -> io::Result<()> {
    let registry = load_registry()?;
    match registry.get(name) {
        Some(p) => {
            println!("Provider      : {}", p.name);
            println!("{}", "─".repeat(50));
            println!("Kind          : {}", p.kind);
            println!("Repository    : {}", p.repository);
            match (&p.password.env, &p.password.file) {
                (Some(env), _) => println!("Password      : env:{}", env),
                (_, Some(file)) => println!("Password      : file:{}", file),
                _ =>              println!("Password      : (none configured)"),
            }
            println!();
            println!("Verification");
            println!("  max snapshot age : {}",
                p.verify.max_snapshot_age_hours
                    .map(|h| format!("{}h", h))
                    .unwrap_or_else(|| "not set".to_owned()));
            println!("  test restore     : {}", p.verify.test_restore);
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No provider found with name '{}'.", name),
            ));
        }
    }
    Ok(())
}

/// Print all registered providers as a summary table.
pub fn list_providers() -> io::Result<()> {
    let registry = load_registry()?;
    if registry.is_empty() {
        println!("No providers registered.");
        println!("Add one with: rehearsa provider add <name> --kind restic --repo <path>");
        return Ok(());
    }

    println!("{:<20} {:<10} {}", "Name", "Kind", "Repository");
    println!("{}", "─".repeat(60));
    let mut entries: Vec<&ProviderConfig> = registry.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for p in entries {
        println!("{:<20} {:<10} {}", p.name, p.kind, p.repository);
    }
    Ok(())
}

/// Remove a provider by name.
pub fn delete_provider(name: &str) -> io::Result<()> {
    let mut registry = load_registry()?;
    if registry.remove(name).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No provider found with name '{}'.", name),
        ));
    }
    save_registry(&registry)?;
    println!("Provider '{}' deleted.", name);
    Ok(())
}

/// Verify a provider's repository is reachable and (for Restic) contains at least one snapshot.
/// Model A: checks repo accessibility via `restic snapshots`.
/// Model B scaffold: snapshot age and test-restore enforcement live here when implemented.
pub fn verify_provider(name: &str) -> io::Result<()> {
    let registry = load_registry()?;
    let provider = match registry.get(name) {
        Some(p) => p,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No provider found with name '{}'.", name),
            ));
        }
    };

    println!("Verifying provider '{}'...", provider.name);
    println!("{}", "─".repeat(50));

    match provider.kind {
        ProviderKind::Restic => verify_restic(provider),
        ProviderKind::Borg   => verify_borg(provider),
    }
}

fn verify_restic(provider: &ProviderConfig) -> io::Result<()> {
    // Build the base command
    let mut cmd = Command::new("restic");
    cmd.arg("--repo").arg(&provider.repository);
    cmd.arg("snapshots").arg("--last").arg("--json");

    // Inject credentials
    match (&provider.password.env, &provider.password.file) {
        (Some(env_var), _) => {
            cmd.env("RESTIC_PASSWORD_ENV", env_var);
            // Pass the actual value if available in the current environment
            if let Ok(val) = std::env::var(env_var) {
                cmd.env("RESTIC_PASSWORD", val);
            }
        }
        (_, Some(file)) => {
            cmd.arg("--password-file").arg(file);
        }
        _ => {
            // No credential config — restic will fall back to its own env lookup
        }
    }

    println!("Repository : {}", provider.repository);
    print!("Reachable  : ");

    let output = cmd.output().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to run restic (is it installed?): {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("✗ FAILED");
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("restic error: {}", stderr.trim()),
        ));
    }

    println!("✓ OK");

    // Parse snapshot list
    let stdout = String::from_utf8_lossy(&output.stdout);
    let snapshots: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Array(vec![]));

    let count = snapshots.as_array().map(|a| a.len()).unwrap_or(0);
    println!("Snapshots  : {}", count);

    if count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No snapshots found in repository. Run a backup first, then re-verify.",
        ));
    }

    // Report most recent snapshot time
    if let Some(latest) = snapshots.as_array().and_then(|a| a.first()) {
        if let Some(time) = latest.get("time").and_then(|t| t.as_str()) {
            println!("Latest     : {}", &time[..19].replace('T', " "));
        }
    }

    // Model B scaffold: age and test-restore checks
    if provider.verify.max_snapshot_age_hours.is_some() || provider.verify.test_restore {
        println!();
        println!("{}", "─".repeat(50));
        if provider.verify.max_snapshot_age_hours.is_some() {
            println!("⚙  Snapshot age enforcement : not yet implemented (scaffolded for Model B)");
        }
        if provider.verify.test_restore {
            println!("⚙  Test restore             : not yet implemented (scaffolded for Model B)");
        }
    }

    println!();
    println!("Status: PROVIDER OK");
    Ok(())
}


fn verify_borg(provider: &ProviderConfig) -> io::Result<()> {
    // borg info <repo> --json — checks reachability and returns repo metadata.
    // borg list <repo> --last 1 --json — confirms at least one archive exists.

    // ── Step 1: repo info (reachability) ─────────────────────────────────
    let mut info_cmd = Command::new("borg");
    info_cmd.arg("info")
            .arg("--json")
            .arg(&provider.repository);

    inject_borg_credentials(&mut info_cmd, provider);

    println!("Repository : {}", provider.repository);
    print!("Reachable  : ");

    let info_out = info_cmd.output().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to run borg (is it installed?): {}", e),
        )
    })?;

    if !info_out.status.success() {
        let stderr = String::from_utf8_lossy(&info_out.stderr);
        println!("✗ FAILED");
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("borg error: {}", stderr.trim()),
        ));
    }

    println!("✓ OK");

    // ── Step 2: archive list (snapshot presence) ──────────────────────────
    let mut list_cmd = Command::new("borg");
    list_cmd.arg("list")
            .arg("--json")
            .arg("--last").arg("1")
            .arg(&provider.repository);

    inject_borg_credentials(&mut list_cmd, provider);

    let list_out = list_cmd.output().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Failed to run borg list: {}", e),
        )
    })?;

    if !list_out.status.success() {
        let stderr = String::from_utf8_lossy(&list_out.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("borg list error: {}", stderr.trim()),
        ));
    }

    // borg list --json returns { "archives": [ { "name": "...", "time": "..." }, ... ] }
    let stdout   = String::from_utf8_lossy(&list_out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);

    let archives = parsed
        .get("archives")
        .and_then(|a| a.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    println!("Archives   : {}", archives);

    if archives == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "No archives found in repository. Run a backup first, then re-verify.",
        ));
    }

    // Report most recent archive
    if let Some(latest) = parsed
        .get("archives")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
    {
        if let Some(name) = latest.get("name").and_then(|n| n.as_str()) {
            println!("Latest     : {}", name);
        }
        if let Some(time) = latest.get("time").and_then(|t| t.as_str()) {
            // Borg timestamps: "2026-02-23T03:00:01.123456" — trim to seconds
            let trimmed = if time.len() >= 19 { &time[..19] } else { time };
            println!("Time       : {}", trimmed.replace('T', " "));
        }
    }

    // Model B scaffold: age enforcement lives here when implemented
    if provider.verify.max_snapshot_age_hours.is_some() || provider.verify.test_restore {
        println!();
        println!("{}", "─".repeat(50));
        if provider.verify.max_snapshot_age_hours.is_some() {
            println!("⚙  Snapshot age enforcement : not yet implemented (scaffolded for Model B)");
        }
        if provider.verify.test_restore {
            println!("⚙  Test restore             : not yet implemented (scaffolded for Model B)");
        }
    }

    println!();
    println!("Status: PROVIDER OK");
    Ok(())
}

/// Inject Borg passphrase credentials into a Command.
/// Borg uses BORG_PASSPHRASE (env var) or BORG_PASSPHRASE_FD / --passphrase-file.
/// We map Rehearsa's PasswordSource onto Borg's native env vars to keep it
/// consistent with how Restic credentials are handled.
fn inject_borg_credentials(cmd: &mut Command, provider: &ProviderConfig) {
    match (&provider.password.env, &provider.password.file) {
        (Some(env_var), _) => {
            // env_var is the *name* of the variable holding the passphrase.
            // Read it from the current process environment and set BORG_PASSPHRASE.
            if let Ok(val) = std::env::var(env_var) {
                cmd.env("BORG_PASSPHRASE", val);
            }
        }
        (_, Some(file)) => {
            // Borg doesn't have a direct --passphrase-file flag, but supports
            // BORG_PASSCOMMAND. We use `cat <file>` as the passcommand.
            cmd.env("BORG_PASSCOMMAND", format!("cat {}", file));
        }
        _ => {
            // No credentials configured. Borg will use its own environment
            // fallback (BORG_PASSPHRASE if set externally, or prompt if interactive).
        }
    }
}

// ======================================================
// PUBLIC LOADER (used by daemon + engine)
// ======================================================

/// Load a single provider by name. Returns None if not found or registry missing.
pub fn load_provider(name: &str) -> Option<ProviderConfig> {
    load_registry().ok()?.remove(name)
}
