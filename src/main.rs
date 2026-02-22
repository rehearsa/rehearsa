mod engine;
mod docker;
mod lock;
mod history;
mod policy;

use clap::{Parser, Subcommand};
use std::process::exit;

use engine::stack::{test_stack, PullPolicy};
use policy::{StackPolicy, save_policy, show_policy, delete_policy};

// ======================================================
// CLI
// ======================================================

#[derive(Parser)]
#[command(name = "rehearsa")]
#[command(about = "Restore rehearsal engine for Docker environments")]
struct Cli {

    /// Output JSON instead of human readable
    #[arg(long)]
    json: bool,

    /// Timeout in seconds
    #[arg(long, default_value_t = 30)]
    timeout: u64,

    /// Inject failure into a specific service (stack mode only)
    #[arg(long)]
    inject_failure: Option<String>,

    /// Strict integrity mode
    #[arg(long)]
    strict_integrity: bool,

    /// Image pull policy: always | if-missing | never
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
        block_on_regression: Option<bool>,

        #[arg(long)]
        fail_on_new_service_failure: Option<bool>,

        // NEW FIELDS
        #[arg(long)]
        fail_on_duration_spike: Option<bool>,

        #[arg(long)]
        duration_spike_percent: Option<u32>,
    },
    Show {
        stack: String,
    },
    Delete {
        stack: String,
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
        "never" => PullPolicy::Never,
        _ => PullPolicy::IfMissing,
    };

    match cli.command {

        // ==================================================
        // STACK
        // ==================================================
        Commands::Stack { command } => match command {
            StackCommands::Test { compose_file } => {

                match test_stack(
                    &compose_file,
                    cli.timeout,
                    cli.json,
                    cli.inject_failure.clone(),
                    cli.strict_integrity,
                    pull_policy,
                ).await {

                    Ok(_) => {}
                    Err(e) => {
                        if cli.json {
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
                block_on_regression,
                fail_on_new_service_failure,
                fail_on_duration_spike,
                duration_spike_percent,
            } => {

                let policy = StackPolicy {
                    min_confidence,
                    block_on_regression,
                    fail_on_new_service_failure,
                    fail_on_duration_spike,
                    duration_spike_percent,
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
        // STATUS
        // ==================================================
        Commands::Status => {
            if let Err(e) = history::status_all() {
                eprintln!("Status error: {}", e);
                exit(1);
            }
        },

        // ==================================================
        // VERSION
        // ==================================================
        Commands::Version => {
            println!("Rehearsa v0.1.0 (B-stage build)");
        }
    }
}
