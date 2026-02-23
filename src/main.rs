mod engine;
mod docker;
mod lock;
mod history;
mod policy;
mod baseline;
mod daemon;

use clap::{Parser, Subcommand};
use std::process::exit;

use engine::stack::{test_stack, PullPolicy};
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
    Status,
    Version,
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
    },
    Unwatch {
        stack: String,
    },
    List,
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
                    Ok(_) => {}
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
                        stack: stack_name.clone(),
                        expected_services: latest.services.keys().cloned().collect(),
                        expected_confidence: latest.confidence,
                        expected_readiness: latest.readiness,
                        expected_duration: latest.duration_seconds,
                        service_scores: latest.services,
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
            DaemonCommands::Watch { stack, compose_file } => {
                if let Err(e) = daemon::add_watch(&stack, &compose_file) {
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
        // VERSION
        // ==================================================

        Commands::Version => {
            println!("rehearsa {}", env!("CARGO_PKG_VERSION"));
        }
    }
}
