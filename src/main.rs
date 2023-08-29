use clap::{Parser, Subcommand};
use anyhow::Result;
use structopt::StructOpt;
#[allow(unused_imports)]
use log::{info, trace, debug};

use scidataflow::lib::project::Project;
use scidataflow::logging_setup::setup;

pub mod logging_setup;

const INFO: &str = "\
SciDataFlow: Manage and Share Scientific Data
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
        #[structopt(name = "filenames", required = true)]
        filenames: Vec<String>,
    },
     #[structopt(name = "config")]
    /// Set local configuration settings (e.g. name), which 
    /// can be propagated to some APIs.
    Config {
        /// Your name (required if not previously set)
        #[structopt(long)]
        name: Option<String>,
        #[structopt(long)]
        email: Option<String>,
        #[structopt(long)]
        affiliation: Option<String>,
    },
    #[structopt(name = "init")]
    /// Initialize a new project.
    Init {
        /// project name (default: the name of the directory)
        #[structopt(long)]
        name: Option<String>
    },

    #[structopt(name = "status")]
    /// Show status of data.
    Status {
        /// Show remotes status
        #[structopt(long)]
        remotes: bool
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
        #[structopt(long)]
        name: Option<String>,

        /// don't initialize remote, only add to manifest
        #[structopt(long)]
        link_only: bool

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
        // Overwrite local files?
        #[structopt(long)]
        overwrite: bool,
    },

    #[structopt(name = "pull")]
    /// Pull in all tracked files from the remote.
    Pull {
        // Overwrite local files?
        #[structopt(long)]
        overwrite: bool,

        // multiple optional directories
        //directories: Vec<PathBuf>,
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
    setup();
    match run().await {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {:?}", e);
            std::process::exit(1);
        }
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Add { filenames }) => {
            let mut proj = Project::new()?;
            proj.add(filenames)
        }
        Some(Commands::Config { name, email, affiliation }) => {
            Project::set_config(name, email, affiliation)
        }
        Some(Commands::Init { name }) => {
            Project::init(name.clone())
        }
        Some(Commands::Status { remotes }) => {
            let mut proj = Project::new()?;
            proj.status(*remotes).await
        }
        Some(Commands::Stats {  }) => {
            //let proj = Project::new()?;
            //proj.stats()
            Ok(())
        }
        Some(Commands::Update { filename }) => {
            let mut proj = Project::new()?;
            proj.update(filename.as_ref())
        }
        Some(Commands::Link { dir, service, key, name, link_only }) => {
            let mut proj = Project::new()?;
            proj.link(dir, service, key, name, link_only).await
        }
        Some(Commands::Ls {}) => {
            let mut proj = Project::new()?;
            proj.ls().await
        },
        Some(Commands::Track { filename }) => {
            let mut proj = Project::new()?;
            proj.track(filename)
        },
        Some(Commands::Untrack { filename }) => {
            let mut proj = Project::new()?;
            proj.untrack(filename)
        },
        Some(Commands::Push { overwrite }) => {
            let mut proj = Project::new()?;
            proj.push(*overwrite).await
        },
        Some(Commands::Pull { overwrite }) => {
            let mut proj = Project::new()?;
            proj.pull(*overwrite).await
        },
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }


}
