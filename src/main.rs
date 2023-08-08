use clap::{Parser, Subcommand};
use reqwest::dns::Resolve;
use anyhow::Result;
use crate::project::Project;
use structopt::StructOpt;
use log::{info, trace, debug};
use tokio;

pub mod utils;
pub mod project;
pub mod data;
pub mod traits;
pub mod remote;

const INFO: &str = "\
SciFlow: Manage and Share Scientific Data
usage: scf [--help] <subcommand>

Some examples:

  # initialize a new project
  scf init

  # get data status
  scf status

  # get data sizes, etc.
  scf stats

  # pull in data
  scf pull
 
  # link the directory data/supplement/ to figshare 
  # use the given token
  scf link  data/supplement figshare <token> [--name figshare project name]

";


#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand,StructOpt)]
enum Commands {
     #[structopt(name = "add")]
    /// Add a data file to the manifest.
    Add {
        /// the file to begin tracking.
        filename: String,
    },

    #[structopt(name = "init")]
    /// Initialize a new project.
    Init {
    },

    #[structopt(name = "status")]
    /// Show status of data.
    Status {
    },

    #[structopt(name = "stats")]
    /// Show status of data.
    Stats {
    },

    #[structopt(name = "update")]
    /// Update MD5s
    Update {
        /// Which file to update (if not set, all tracked files are update).
        filename: Option<String>,
    },

    #[structopt(name = "link")]
    /// Link a directory to a remote storage solution.
    Link {
        /// directory to link to remote storage.
        dir: String,
        /// the service to use (currently only FigShare).
        service: String,
        /// the authentication token.
        key: String,
        /// project name for remote (default: directory name)
        name: Option<String>
    },

    #[structopt(name = "ls")]
    /// List remotes.
    Ls {
    },

    #[structopt(name = "untrack")]
    /// No longer keep track of this file on the remote.
    Untrack {
        /// the file to untrack with remote.
        filename: String
    },

    #[structopt(name = "track")]
    /// Keep track of this file on the remote.
    Track {
        /// the file to track with remote.
        filename: String
    },

    #[structopt(name = "push")]
    /// Push all tracked files to remote.
    Push {
    },

}

pub fn print_errors(response: Result<()>) {
    match response {
        Ok(_) => {},
        Err(err) => eprintln!("Error: {}", err),
    }
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}", e);
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Add { filename }) => {
            let mut proj = Project::new()?;
            proj.add(filename)
        }
        Some(Commands::Init {  }) => {
            Project::init()
        }
        Some(Commands::Status {  }) => {
            let proj = Project::new()?;
            proj.status()
        }
        Some(Commands::Stats {  }) => {
            let proj = Project::new()?;
            proj.stats()
        }
        Some(Commands::Update { filename }) => {
            let mut proj = Project::new()?;
            proj.update(filename.as_ref())
        }
        Some(Commands::Link { dir, service, key, name }) => {
            let mut proj = Project::new()?;
            proj.link(dir, service, key, name).await
        }
        Some(Commands::Ls {}) => {
            let mut proj = Project::new()?;
            proj.ls().await
        },
        Some(Commands::Track { filename }) => {
            let mut proj = Project::new()?;
            proj.track(filename)
        }
        Some(Commands::Untrack { filename }) => {
            let mut proj = Project::new()?;
            proj.untrack(filename)
        }
        Some(Commands::Push {}) => {
            let mut proj = Project::new()?;
            proj.push().await
        }
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }


}
