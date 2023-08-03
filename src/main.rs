use clap::{Parser, Subcommand};
use crate::project::Project;
use structopt::StructOpt;

pub mod utils;
pub mod project;
pub mod data;
pub mod traits;
pub mod remote;


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


fn main() {
    env_logger::init();


    let cli = Cli::parse();


    match &cli.command {
        Some(Commands::Add { filename }) => {
            let mut dc = Project::new();
            dc.add(filename);
        }
        Some(Commands::Init {  }) => {
            Project::init();
        }
        Some(Commands::Status {  }) => {
            let dc = Project::new();
            dc.status();
        }
        Some(Commands::Touch { filename }) => {
            let mut dc = Project::new();
            dc.touch(filename.as_ref());
        }
        Some(Commands::Link { dir, service, key }) => {
            let mut dc = Project::new();
            dc.link(dir, service, key)
        }
        None => {}
    }


}
