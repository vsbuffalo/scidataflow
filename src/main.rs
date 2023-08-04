use clap::{Parser, Subcommand};
use reqwest::dns::Resolve;
use crate::project::Project;
use structopt::StructOpt;

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

    #[structopt(name = "touch")]
    /// Update modification times.
    Touch {
        /// Which file to touch (if not set, all tracked files are touched).
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
    }
}

pub fn print_errors(response: Result<(), String>) {
    match response {
        Ok(_) => {},
        Err(err) => println!("Error: {}", err),
    }
}

fn main() {
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
            if let Ok(proj) = Project::new() {
                print_errors(proj.status());
            }

        }
        Some(Commands::Stats {  }) => {
            if let Ok(proj) = Project::new() {
                print_errors(proj.stats());
            }

        }
        Some(Commands::Touch { filename }) => {
            if let Ok(mut proj) = Project::new() {
                print_errors(proj.touch(filename.as_ref()));
            }
        }
        Some(Commands::Link { dir, service, key }) => {
            if let Ok(mut proj) = Project::new() {
                print_errors(proj.link(dir, service, key));
            }

        }
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }


}
