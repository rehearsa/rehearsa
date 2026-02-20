mod docker;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rehearsa")]
#[command(about = "Restore rehearsal engine for Docker environments")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test restore capability of a container
    Test {
        /// Name of the container to test
        container: String,
    },
    /// List detected containers
    List,
    /// Show version information
    Version,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test { container } => {
            println!("Testing container: {}", container);
        }
        Commands::List => {
    if let Err(e) = docker::list_containers().await {
        eprintln!("Error listing containers: {}", e);
    }
}
        Commands::Version => {
            println!("Rehearsa v0.1.0 (early development)");
        }
    }
}
