mod engine;
mod docker;
mod lock;
mod history;
mod policy;
mod baseline;
mod daemon;
mod provider;
mod notify;
mod report;
mod coverage;

use clap::{Parser, Subcommand};
use chrono;
use std::process::exit;

use engine::stack::{test_stack, PullPolicy, cleanup_orphans};
use policy::{StackPolicy, save_policy, show_policy, delete_policy};
use baseline::{
    StackBaseline,
    save_baseline,
    load_baseline,
    delete_baseline,
    compare_to_baseline,
};

// ======================================================
// CLI
// ======================================================

#[derive(Parser)]
#[command(name = "rehearsa")]
#[command(about = "Restore rehearsal engine for Docker environments")]
struct Cli {
    #[arg(long)]
    json: bool,

    #[arg(long)]
    ci: bool,

    #[arg(long, default_value_t = 30)]
    timeout: u64,

    #[arg(long)]
    inject_failure: Option<String>,

    #[arg(long)]
    strict_integrity: bool,

    #[arg(long, default_value = "if-missing")]
    pull: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Stack {
        #[command(subcommand)]
        command: StackCommands,
    },
    History {
        #[command(subcommand)]
        command: HistoryCommands,
    },
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },
    Baseline {
        #[command(subcommand)]
        command: BaselineCommands,
    },
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    Notify {
        #[command(subcommand)]
        command: NotifyCommands,
    },
    Status,
    /// Show fleet-wide restore contract coverage across all watched stacks.
    Coverage {
        /// Output raw JSON instead of the formatted table.
        #[arg(long)]
        json: bool,
    },
    Version,
    /// Remove orphaned containers and networks left by crashed rehearsals.
    Cleanup,
    /// Generate a compliance report for a stack or the full fleet.
    Report {
        /// Stack name. Omit for a fleet-wide report covering all stacks.
        #[arg(long)]
        stack: Option<String>,

        /// Output format: json | pdf | both  [default: both]
        #[arg(long, default_value = "both")]
        format: String,

        /// Output path or directory. Defaults to stdout (JSON) or ./<stack>-report.pdf (PDF).
        #[arg(long)]
        output: Option<String>,

        /// Named backup provider to include in the report.
        #[arg(long)]
        provider: Option<String>,

        /// Number of historical runs to include in the trend section.
        #[arg(long, default_value = "10")]
        window: usize,
    },
}

#[derive(Subcommand)]
enum StackCommands {
    Test {
        compose_file: String,
    },
}

#[derive(Subcommand)]
enum HistoryCommands {
    List,
    Show {
        stack: String,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    Set {
        stack: String,

        #[arg(long)]
        min_confidence: Option<u32>,

        #[arg(long)]
        min_readiness: Option<u32>,

        #[arg(long)]
        block_on_regression: Option<bool>,

        #[arg(long)]
        fail_on_new_service_failure: Option<bool>,

        #[arg(long)]
        fail_on_duration_spike: Option<bool>,

        #[arg(long)]
        duration_spike_percent: Option<u32>,

        #[arg(long)]
        fail_on_baseline_drift: Option<bool>,
    },
    Show {
        stack: String,
    },
    Delete {
        stack: String,
    },
}

#[derive(Subcommand)]
enum BaselineCommands {
    Set {
        compose_file: String,
    },
    Show {
        stack: String,
    },
    Diff {
        stack: String,
    },
    Delete {
        stack: String,
    },
    /// Promote a historical run to the current baseline.
    /// Defaults to the latest run if --timestamp is not specified.
    Promote {
        stack: String,
        /// Timestamp of the specific run to promote (partial match supported).
        /// Run `rehearsa history show <stack>` to list available timestamps.
        #[arg(long)]
        timestamp: Option<String>,
    },
    /// Show baseline version history.
    /// Omit --stack to see all stacks; provide --stack for per-version diffs.
    History {
        #[arg(long)]
        stack: Option<String>,
    },
    /// Rehearse all watched stacks and pin initial baselines.
    /// Run once after first install to establish a starting contract for every stack.
    AutoInit,
}

#[derive(Subcommand)]
enum DaemonCommands {
    Install,
    Uninstall,
    Status,
    Run,
    Watch {
        stack: String,
        compose_file: String,
        /// Cron expression for scheduled rehearsals, e.g. "0 3 * * *"
        #[arg(long)]
        schedule: Option<String>,
        /// Run a rehearsal on daemon start if a scheduled window was missed
        #[arg(long, default_value_t = false)]
        catch_up: bool,
        /// Named backup provider to associate with this stack (see: rehearsa provider list)
        #[arg(long)]
        provider: Option<String>,
        /// Named notify channel override for this stack (see: rehearsa notify list)
        #[arg(long)]
        notify: Option<String>,
    },
    Unwatch {
        stack: String,
    },
    List,
    /// Set the maximum number of simultaneous rehearsals.
    /// Restart the daemon after changing.
    SetConcurrency {
        /// Maximum concurrent rehearsals (minimum: 1, recommended: 1 for low-power hardware)
        limit: usize,
    },
    /// Show current daemon configuration and resolved settings.
    Config,
}

#[derive(Subcommand)]
enum NotifyCommands {
    /// Register a new webhook notification channel
    Add {
        /// Unique name for this channel (e.g. slack-ops, discord-alerts)
        name: String,
        /// Webhook URL to POST notifications to
        #[arg(long)]
        url: String,
        /// Optional secret sent as X-Rehearsa-Secret header
        #[arg(long)]
        secret: Option<String>,
    },
    /// Show config for a notify channel
    Show {
        name: String,
    },
    /// List all registered notify channels
    List,
    /// Remove a notify channel
    Delete {
        name: String,
    },
    /// Set the global default notify channel
    Default {
        name: String,
    },
    /// Send a test notification to verify delivery
    Test {
        name: String,
    },
    /// Add or update the email transport on a channel
    AddEmail {
        /// Channel name (creates the channel if it doesn't exist)
        name: String,
        /// Email provider: smtp | sendgrid  [default: smtp]
        #[arg(long, default_value = "smtp")]
        provider: String,
        /// From address, e.g. "Rehearsa <alerts@example.com>"
        #[arg(long)]
        from: String,
        /// Recipient address (repeatable: --to a@b.com --to c@d.com)
        #[arg(long, required = true)]
        to: Vec<String>,
        /// SMTP server hostname
        #[arg(long)]
        smtp_host: Option<String>,
        /// SMTP server port [default: 587]
        #[arg(long)]
        smtp_port: Option<u16>,
        /// SMTP username
        #[arg(long)]
        smtp_username: Option<String>,
        /// SMTP password (literal). Prefer --smtp-password-env in production.
        #[arg(long)]
        smtp_password: Option<String>,
        /// Environment variable holding the SMTP password
        #[arg(long)]
        smtp_password_env: Option<String>,
        /// Disable STARTTLS (not recommended; for local relays only)
        #[arg(long, default_value_t = true)]
        smtp_starttls: bool,
        /// Sendgrid API key (literal). Prefer --sendgrid-api-key-env.
        #[arg(long)]
        sendgrid_api_key: Option<String>,
        /// Environment variable holding the Sendgrid API key
        #[arg(long)]
        sendgrid_api_key_env: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// Register a new backup provider
    Add {
        /// Unique name for this provider (e.g. restic-main, client-a-restic)
        name: String,

        /// Provider type. Supported: restic
        #[arg(long)]
        kind: String,

        /// Repository path or URI (e.g. /mnt/backups or s3:bucket/path)
        #[arg(long)]
        repo: String,

        /// Environment variable that holds the repository password
        #[arg(long)]
        password_env: Option<String>,

        /// Path to a file containing the repository password
        #[arg(long)]
        password_file: Option<String>,
    },
    /// Show full config for a provider
    Show {
        name: String,
    },
    /// List all registered providers
    List,
    /// Remove a provider
    Delete {
        name: String,
    },
    /// Verify a provider's repository is reachable and has snapshots
    Verify {
        name: String,
    },
    /// Set Model B verification options on a provider
    VerifySet {
        name: String,
        /// Maximum snapshot age in hours before verification fails
        #[arg(long)]
        max_age_hours: Option<u64>,
        /// Require a test restore before each rehearsal
        #[arg(long, default_value_t = false)]
        test_restore: bool,
    },
}

// ======================================================
// MAIN
// ======================================================

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let pull_policy = match cli.pull.as_str() {
        "always" => PullPolicy::Always,
        "never"  => PullPolicy::Never,
        _        => PullPolicy::IfMissing,
    };

    match cli.command {

        // ==================================================
        // STACK
        // ==================================================

        Commands::Stack { command } => match command {
            StackCommands::Test { compose_file } => {
                let json_mode = cli.json || cli.ci;

                match test_stack(
                    &compose_file,
                    cli.timeout,
                    json_mode,
                    cli.inject_failure.clone(),
                    cli.strict_integrity,
                    pull_policy,
                ).await {
                    Ok(summary) => {
                        if summary.policy_violated {
                            exit(4);
                        } else if summary.baseline_drift {
                            exit(5);
                        } else if summary.confidence < 40 {
                            exit(3);
                        } else if summary.confidence < 70 {
                            exit(2);
                        }
                    }
                    Err(e) => {
                        if json_mode {
                            println!(
                                r#"{{"stack":"{}","fatal_error":"{}"}}"#,
                                compose_file, e
                            );
                        } else {
                            eprintln!("Stack Restore Simulation: FAILED");
                            eprintln!("Fatal Error: {}", e);
                        }
                        exit(1);
                    }
                }
            }
        },

        // ==================================================
        // HISTORY
        // ==================================================

        Commands::History { command } => match command {
            HistoryCommands::List => {
                if let Err(e) = history::list_stacks() {
                    eprintln!("History error: {}", e);
                    exit(1);
                }
            }
            HistoryCommands::Show { stack } => {
                if let Err(e) = history::show_stack(&stack) {
                    eprintln!("History error: {}", e);
                    exit(1);
                }
            }
        },

        // ==================================================
        // POLICY
        // ==================================================

        Commands::Policy { command } => match command {
            PolicyCommands::Set {
                stack,
                min_confidence,
                min_readiness,
                block_on_regression,
                fail_on_new_service_failure,
                fail_on_duration_spike,
                duration_spike_percent,
                fail_on_baseline_drift,
            } => {
                let policy = StackPolicy {
                    min_confidence,
                    min_readiness,
                    block_on_regression,
                    fail_on_new_service_failure,
                    fail_on_duration_spike,
                    duration_spike_percent,
                    fail_on_baseline_drift,
                };

                if let Err(e) = save_policy(&stack, &policy) {
                    eprintln!("Policy error: {}", e);
                    exit(1);
                }

                println!("Policy saved for stack '{}'", stack);
            }

            PolicyCommands::Show { stack } => {
                if let Err(e) = show_policy(&stack) {
                    eprintln!("Policy error: {}", e);
                    exit(1);
                }
            }

            PolicyCommands::Delete { stack } => {
                if let Err(e) = delete_policy(&stack) {
                    eprintln!("Policy error: {}", e);
                    exit(1);
                }
                println!("Policy deleted for stack '{}'", stack);
            }
        },

        // ==================================================
        // BASELINE
        // ==================================================

        Commands::Baseline { command } => match command {

            BaselineCommands::Set { compose_file } => {
                let stack_name = std::path::Path::new(&compose_file)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();

                if let Some(latest) = history::load_latest(&stack_name) {
                    let baseline = StackBaseline {
                        schema_version: history::CURRENT_SCHEMA_VERSION,
                        stack: stack_name.clone(),
                        expected_services: latest.services.keys().cloned().collect(),
                        expected_confidence: latest.confidence,
                        expected_readiness: latest.readiness,
                        expected_duration: latest.duration_seconds,
                        service_scores: latest.services,
                        pinned_at: Some(latest.timestamp.clone()),
                        promoted_at: Some(chrono::Utc::now().to_rfc3339()),
                    };

                    if let Err(e) = save_baseline(&stack_name, &baseline) {
                        eprintln!("Baseline error: {}", e);
                        exit(1);
                    }

                    println!("Baseline saved for stack '{}'", stack_name);
                } else {
                    eprintln!(
                        "No history found for stack '{}'. Run a test first.",
                        stack_name
                    );
                    exit(1);
                }
            }

            BaselineCommands::Show { stack } => {
                if let Some(b) = load_baseline(&stack) {
                    println!("Restore Contract: {}", b.stack);
                    println!("{}", "─".repeat(50));
                    println!("Confidence floor : {}%", b.expected_confidence);
                    if let Some(r) = b.expected_readiness {
                        println!("Readiness floor  : {}%", r);
                    }
                    println!("Duration ceiling : {}s", b.expected_duration);
                    println!(
                        "Services         : {}",
                        b.expected_services.join(", ")
                    );
                    println!();
                    println!("Service Scores:");
                    for (svc, score) in &b.service_scores {
                        println!("  {:<20} {}%", svc, score);
                    }
                } else {
                    println!("No baseline found for '{}'", stack);
                }
            }

            BaselineCommands::Diff { stack } => {
                let baseline = match load_baseline(&stack) {
                    Some(b) => b,
                    None => {
                        eprintln!(
                            "No baseline found for '{}'. Run: rehearsa baseline set {}.yml",
                            stack, stack
                        );
                        exit(1);
                    }
                };

                let latest = match history::load_latest(&stack) {
                    Some(r) => r,
                    None => {
                        eprintln!("No history found for '{}'.", stack);
                        exit(1);
                    }
                };

                let drift = compare_to_baseline(
                    &baseline,
                    &latest.services,
                    latest.confidence,
                    latest.readiness,
                    latest.duration_seconds,
                );

                println!("Baseline Diff: {}", stack);
                println!("{}", "─".repeat(60));
                println!(
                    "{:<20} {:<12} {:<12} {}",
                    "Metric", "Contract", "Current", "Delta"
                );
                println!("{}", "─".repeat(60));

                let conf_delta_str = if drift.confidence_delta > 0 {
                    format!("+{}%  ✓", drift.confidence_delta)
                } else if drift.confidence_delta < 0 {
                    format!("{}%  ⚠", drift.confidence_delta)
                } else {
                    "0      ✓".to_string()
                };

                println!(
                    "{:<20} {:<12} {:<12} {}",
                    "Confidence",
                    format!("{}%", baseline.expected_confidence),
                    format!("{}%", latest.confidence),
                    conf_delta_str
                );

                if let (Some(rd), Some(be)) = (
                    drift.readiness_delta,
                    baseline.expected_readiness,
                ) {
                    let r_str = if rd < 0 {
                        format!("{}%  ⚠", rd)
                    } else {
                        format!("+{}%  ✓", rd)
                    };
                    println!(
                        "{:<20} {:<12} {:<12} {}",
                        "Readiness",
                        format!("{}%", be),
                        format!("{}%", latest.readiness.unwrap_or(0)),
                        r_str
                    );
                }

                if let Some(dd) = drift.duration_delta_percent {
                    let d_str = if dd > 20 {
                        format!("+{}%  ⚠", dd)
                    } else {
                        format!("+{}%  ✓", dd)
                    };
                    println!(
                        "{:<20} {:<12} {:<12} {}",
                        "Duration",
                        format!("{}s", baseline.expected_duration),
                        format!("{}s", latest.duration_seconds),
                        d_str
                    );
                }

                if !drift.new_services.is_empty() {
                    println!("\n⚠  New services    : {}", drift.new_services.join(", "));
                }
                if !drift.missing_services.is_empty() {
                    println!("⚠  Missing services: {}", drift.missing_services.join(", "));
                }

                let has_drift = drift.confidence_delta < 0
                    || !drift.new_services.is_empty()
                    || !drift.missing_services.is_empty()
                    || drift.duration_delta_percent.unwrap_or(0) > 20;

                println!();
                if has_drift {
                    println!("Status: DRIFT DETECTED");
                    exit(2);
                } else {
                    println!("Status: CONTRACT HONOURED");
                }
            }

            BaselineCommands::Delete { stack } => {
                if let Err(e) = delete_baseline(&stack) {
                    eprintln!("Baseline error: {}", e);
                    exit(1);
                }
                println!("Baseline deleted for '{}'", stack);
            }

            BaselineCommands::Promote { stack, timestamp } => {
                if let Err(e) = baseline::promote_baseline(&stack, timestamp.as_deref()) {
                    eprintln!("Baseline error: {}", e);
                    exit(1);
                }
            }

            BaselineCommands::History { stack } => {
                let result = match stack {
                    Some(ref s) => baseline::show_stack_baseline_history(s),
                    None        => baseline::show_all_baseline_history(),
                };
                if let Err(e) = result {
                    eprintln!("Baseline error: {}", e);
                    exit(1);
                }
            }

            BaselineCommands::AutoInit => {
                let registry = match daemon::load_registry() {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to load watch registry: {}", e);
                        exit(1);
                    }
                };

                if registry.watches.is_empty() {
                    eprintln!("No watched stacks found. Add stacks with: rehearsa daemon watch <stack> <compose-file>");
                    exit(1);
                }

                println!("Rehearsa Baseline Auto-Init");
                println!("{}", "─".repeat(60));
                println!("Running initial rehearsal for {} stack(s)...", registry.watches.len());
                println!("Note: these baselines reflect current infrastructure state.");
                println!("      Review scores and adjust policy before relying on them.");
                println!();

                let mut passed = 0;
                let mut failed = 0;

                for watch in &registry.watches {
                    print!("  {} ... ", watch.stack);

                    match test_stack(
                        &watch.compose_path,
                        120,
                        false,
                        None,
                        false,
                        PullPolicy::IfMissing,
                    ).await {
                        Ok(summary) => {
                            // Pin whatever came back as the initial baseline
                            if let Some(latest) = history::load_latest(&watch.stack) {
                                let b = baseline::StackBaseline {
                                    schema_version: history::CURRENT_SCHEMA_VERSION,
                                    stack: watch.stack.clone(),
                                    expected_services: latest.services.keys().cloned().collect(),
                                    expected_confidence: latest.confidence,
                                    expected_readiness: latest.readiness,
                                    expected_duration: latest.duration_seconds,
                                    service_scores: latest.services,
                                    pinned_at: Some(latest.timestamp.clone()),
                                    promoted_at: Some(chrono::Utc::now().to_rfc3339()),
                                };
                                match baseline::save_baseline(&watch.stack, &b) {
                                    Ok(_) => {
                                        println!("✓ confidence {}%  readiness {}%  [INITIAL BASELINE SET]",
                                            summary.confidence, summary.readiness);
                                        passed += 1;
                                    }
                                    Err(e) => {
                                        println!("✗ rehearsal ok but baseline save failed: {}", e);
                                        failed += 1;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("✗ rehearsal failed: {}", e);
                            failed += 1;
                        }
                    }
                }

                println!();
                println!("{}", "─".repeat(60));
                println!("Complete: {} pinned, {} failed", passed, failed);
                println!();
                println!("Next steps:");
                println!("  rehearsa status                    — review fleet scores");
                println!("  rehearsa baseline show <stack>     — inspect a contract");
                println!("  rehearsa policy set <stack> ...    — enforce standards");

                if failed > 0 {
                    exit(1);
                }
            }
        },

        // ==================================================
        // DAEMON
        // ==================================================

        Commands::Daemon { command } => match command {
            DaemonCommands::Install => {
                if let Err(e) = daemon::install_daemon() {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Uninstall => {
                if let Err(e) = daemon::uninstall_daemon() {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Status => {
                if let Err(e) = daemon::daemon_status() {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Run => {
                if let Err(e) = daemon::run_daemon().await {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Watch { stack, compose_file, schedule, catch_up, provider, notify } => {
                // Validate the provider name exists before registering the watch
                if let Some(ref pname) = provider {
                    if provider::load_provider(pname).is_none() {
                        eprintln!(
                            "Provider '{}' not found. Register it first with: rehearsa provider add {}",
                            pname, pname
                        );
                        exit(1);
                    }
                }
                // Validate the notify channel exists before registering the watch
                if let Some(ref nchan) = notify {
                    if notify::resolve_channel(Some(nchan)).is_none() {
                        eprintln!(
                            "Notify channel '{}' not found. Register it first with: rehearsa notify add {}",
                            nchan, nchan
                        );
                        exit(1);
                    }
                }
                if let Err(e) = daemon::add_watch(
                    &stack,
                    &compose_file,
                    schedule.as_deref(),
                    catch_up,
                    provider.as_deref(),
                    notify.as_deref(),
                ) {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Unwatch { stack } => {
                if let Err(e) = daemon::remove_watch(&stack) {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::List => {
                if let Err(e) = daemon::list_watches() {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::SetConcurrency { limit } => {
                if let Err(e) = daemon::set_concurrency(limit) {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
            DaemonCommands::Config => {
                if let Err(e) = daemon::show_config() {
                    eprintln!("Daemon error: {}", e);
                    exit(1);
                }
            }
        },

        // ==================================================
        // NOTIFY
        // ==================================================

        Commands::Notify { command } => match command {
            NotifyCommands::Add { name, url, secret } => {
                if let Err(e) = notify::add_channel(&name, &url, secret.as_deref()) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
            NotifyCommands::Show { name } => {
                if let Err(e) = notify::show_channel(&name) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
            NotifyCommands::List => {
                if let Err(e) = notify::list_channels() {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
            NotifyCommands::Delete { name } => {
                if let Err(e) = notify::delete_channel(&name) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
            NotifyCommands::Default { name } => {
                if let Err(e) = notify::set_default(&name) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
            NotifyCommands::Test { name } => {
                if let Err(e) = notify::test_channel(&name) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }

            NotifyCommands::AddEmail {
                name, provider, from, to,
                smtp_host, smtp_port, smtp_username,
                smtp_password, smtp_password_env, smtp_starttls,
                sendgrid_api_key, sendgrid_api_key_env,
            } => {
                let email_provider = match provider.as_str() {
                    "sendgrid" => notify::EmailProvider::Sendgrid,
                    _          => notify::EmailProvider::Smtp,
                };

                if let Err(e) = notify::add_email_channel(
                    &name,
                    email_provider,
                    &from,
                    to,
                    smtp_host.as_deref(),
                    smtp_port,
                    smtp_username.as_deref(),
                    smtp_password.as_deref(),
                    smtp_password_env.as_deref(),
                    smtp_starttls,
                    sendgrid_api_key.as_deref(),
                    sendgrid_api_key_env.as_deref(),
                ) {
                    eprintln!("Notify error: {}", e);
                    exit(1);
                }
            }
        },

        // ==================================================
        // PROVIDER
        // ==================================================

        Commands::Provider { command } => match command {
            ProviderCommands::Add { name, kind, repo, password_env, password_file } => {
                if let Err(e) = provider::add_provider(
                    &name,
                    &kind,
                    &repo,
                    password_env.as_deref(),
                    password_file.as_deref(),
                ) {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
            ProviderCommands::Show { name } => {
                if let Err(e) = provider::show_provider(&name) {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
            ProviderCommands::List => {
                if let Err(e) = provider::list_providers() {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
            ProviderCommands::Delete { name } => {
                if let Err(e) = provider::delete_provider(&name) {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
            ProviderCommands::Verify { name } => {
                if let Err(e) = provider::verify_provider(&name) {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
            ProviderCommands::VerifySet { name, max_age_hours, test_restore } => {
                if let Err(e) = provider::set_provider_verify(&name, max_age_hours, test_restore) {
                    eprintln!("Provider error: {}", e);
                    exit(1);
                }
            }
        },

        // ==================================================
        // STATUS
        // ==================================================

        Commands::Status => {
            if let Err(e) = history::status_all() {
                eprintln!("Status error: {}", e);
                exit(1);
            }
        }

        // ==================================================
        // COVERAGE
        // ==================================================

        Commands::Coverage { json } => {
            match coverage::build_coverage() {
                Ok(summary) => {
                    if json {
                        if let Err(e) = coverage::print_coverage_json(&summary) {
                            eprintln!("Coverage error: {}", e);
                            exit(1);
                        }
                    } else {
                        coverage::print_coverage(&summary);
                        if summary.coverage_pct < 100 {
                            exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Coverage error: {}", e);
                    exit(1);
                }
            }
        }

        // ==================================================
        // REPORT
        // ==================================================

        Commands::Report { stack, format, output, provider, window } => {
            let fmt = match format.as_str() {
                "json" => report::ReportFormat::Json,
                "pdf"  => report::ReportFormat::Pdf,
                _      => report::ReportFormat::Both,
            };

            let args = report::ReportArgs {
                stack,
                format: fmt,
                output,
                provider,
                window,
            };

            if let Err(e) = report::run_report(&args) {
                eprintln!("Error: {}", e);
                exit(1);
            }
        }

        // ==================================================
        // CLEANUP
        // ==================================================

        Commands::Cleanup => {
            println!("Scanning for orphaned rehearsal resources...");
            println!();
            if let Err(e) = cleanup_orphans().await {
                eprintln!("Cleanup error: {}", e);
                exit(1);
            }
        }

        // ==================================================
        // VERSION
        // ==================================================

        Commands::Version => {
            println!("rehearsa {}", env!("CARGO_PKG_VERSION"));
        }
    }
}
