use clap::{Parser, Subcommand};
use crate::project::Project;


pub mod utils;
pub mod project;
pub mod data;
pub mod traits;



#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Add {
        filename: String,
    },

    Init {
    },

    Status {
    },
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
        None => {}
    }


}
