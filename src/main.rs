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
    /// Track a data file.
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


}

pub fn print_errors(response: Result<()>) {
    match response {
        Ok(_) => {},
        Err(err) => eprintln!("Error: {}", err),
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();


    let cli = Cli::parse();


    match &cli.command {
        Some(Commands::Add { filename }) => {
            if let Ok(mut proj) = Project::new() {
                print_errors(proj.add(filename));
            }
        }
        Some(Commands::Init {  }) => {
            print_errors(Project::init());
        }
        Some(Commands::Status {  }) => {
            match Project::new() {
                Ok(proj) => {
                    print_errors(proj.status());
                },
                Err(err) => {
                    println!("Error while creating new project: {}", err);
                }
            }
        }
        Some(Commands::Stats {  }) => {
            if let Ok(proj) = Project::new() {
                print_errors(proj.stats());
            }
        }
        Some(Commands::Update { filename }) => {
            if let Ok(mut proj) = Project::new() {
                print_errors(proj.update(filename.as_ref()));
            }
        }
        Some(Commands::Link { dir, service, key, name }) => {
            match Project::new() {
                Ok(mut proj) => {
                    print_errors(proj.link(dir, service, key, name).await);
                },
                Err(err) => {
                    println!("Error while creating new project: {}", err);
                }
            }
        }
        Some(Commands::Ls {}) => {
            match Project::new() {
                Ok(mut proj) => {
                    print_errors(proj.ls().await);
                },
                Err(err) => {
                    println!("Error while creating new project: {}", err);
                }
            }

        }
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }


}
