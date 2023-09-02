use clap::{Parser, Subcommand};
use anyhow::Result;
use structopt::StructOpt;
#[allow(unused_imports)]
use log::{info, trace, debug};
use tokio::runtime::Builder;


use scidataflow::lib::project::Project;
use scidataflow::logging_setup::setup;

pub mod logging_setup;

const INFO: &str = "\
SciDataFlow: Manage and Share Scientific Data
usage: scf [--help] <subcommand>

Some examples:

  Set up a your user metadata:
  $ sdf config --name \"Joan B. Scientist\" --email \"joanbscientist@berkeley.edu\" 
     --affiliation \"UC Berkeley\"

  Initialize a new project: 
  $ sdf init

  Get data status (use --remotes for remote status and/or --all for all remote files):
  $ sdf status
 
  Link the directory data/supplement/ to FigShare (requires API token):
  $ sdf link  data/supplement FigShare <token> [--name project_name]

  Pull in data (you may want --overwrite):
  $ sdf pull

  Push data to a remote (you may want --overwrite):
  $ sdf pull

  Download a file from a URL and register it in the Data Manifest:
  $ sdf get https://ftp.ensembl.org/some/path/to/large/data.fa.gz

  Bulk download data from a bunch of URLs:
  $ sdf bulk links_to_data.tsv --column 1  # links are in *second* column

See 'sdf --help' or `sdf <subcommand> --help`. Or, see the README at: 
https://github.com/vsbuffalo/scidataflow/.

Please submit bugs or feature requests to: 
https://github.com/vsbuffalo/scidataflow/issues.
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
    /// Set local system-wide metadata (e.g. your name, email, etc.), which 
    /// can be propagated to some APIs.
    Config {
        /// Your name.
        #[structopt(long)]
        name: Option<String>,
        // Your email.
        #[structopt(long)]
        email: Option<String>,
        // Your affiliation.
        #[structopt(long)]
        affiliation: Option<String>,
    },
    #[structopt(name = "init")]
    /// Initialize a new project.
    Init {
        /// Project name (default: the name of the directory).
        #[structopt(long)]
        name: Option<String>
    },
    #[structopt(name = "get")]
    /// Download a file from a URL.
    Get {
        /// Download filename (default: based on URL).
        url: String,
        #[structopt(long)]
        name: Option<String>,
        /// Overwrite local files if they exit.
        #[structopt(long)]
        overwrite: bool
    },
    #[structopt(name = "bulk")]
    /// Download a bunch of files from links stored in a file.
    Bulk {
        /// A TSV or CSV file containing a column of URLs. Type inferred from suffix.
        filename: String,
        /// Which column contains links (default: first).
        #[structopt(long)]
        column: Option<u64>,
        /// The TSV or CSV starts with a header (i.e. skip first line).
        #[structopt(long)]
        header: bool,
        /// Overwrite local files if they exit.
        #[structopt(long)]
        overwrite: bool,
    },
    #[structopt(name = "status")]
    /// Show status of data.
    Status {
        /// Show remotes status (requires network).
        #[structopt(long)]
        remotes: bool, 

        /// Show statuses of all files, including those on remote(s) but not in the manifest.
        #[structopt(long)]
        all: bool

    },

    #[structopt(name = "stats")]
    /// Show file size statistics.
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
        /// Directory to link to remote storage.
        dir: String,
        /// The data repository service to use (either 'figshare' or 'zenodo').
        service: String,
        /// The authentication token.
        key: String,
        /// Project name for remote (default: the metadata title in the data 
        /// manifest, or if that's not set, the directory name).
        #[structopt(long)]
        name: Option<String>,

        /// Don't initialize remote, only add to manifest. This will retrieve
        /// the remote information (i.e. the FigShare Article ID or Zenodo
        /// Depository ID) to add to the manifest. Requires network.
        #[structopt(long)]
        link_only: bool

    },

    #[structopt(name = "untrack")]
    /// No longer keep track of this file on the remote.
    Untrack {
        /// The file to untrack with remote.
        filename: String
    },

    #[structopt(name = "track")]
    /// Keep track of this file on the remote.
    Track {
        /// The file to track with remote.
        filename: String
    },

    #[structopt(name = "push")]
    /// Push all tracked files to remote.
    Push {
        /// Overwrite remote files if they exit.
        #[structopt(long)]
        overwrite: bool,
    },

    #[structopt(name = "pull")]
    /// Pull in all tracked files from the remote.
    Pull {
        /// Overwrite local files if they exit.
        #[structopt(long)]
        overwrite: bool,

        // multiple optional directories
        //directories: Vec<PathBuf>,
    },

     #[structopt(name = "metadata")]
    /// Update the project metadata.
    Metadata {
        /// The project name.
        #[structopt(long)]
        title: Option<String>,
        // A description of the project.
        #[structopt(long)]
        description: Option<String>,
    },
 
}

pub fn print_errors(response: Result<()>) {
    match response {
        Ok(_) => {},
        Err(err) => eprintln!("Error: {}", err),
    }
}

fn main() {
    setup();

    let ncores = 4;

    let runtime = Builder::new_multi_thread()
        .worker_threads(ncores)
        .enable_all()
        .build()
        .unwrap();


    runtime.block_on(async {
        match run().await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }
        }
    });
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Add { filenames }) => {
            let mut proj = Project::new()?;
            proj.add(filenames).await
        }
        Some(Commands::Config { name, email, affiliation }) => {
            Project::set_config(name, email, affiliation)
        }
        Some(Commands::Get { url, name, overwrite }) => {
            let mut proj = Project::new()?;
            proj.get(url, name.as_deref(), *overwrite).await
        }
        Some(Commands::Bulk { filename, column, header, overwrite }) => {
            let mut proj = Project::new()?;
            proj.bulk(filename, *column, *header, *overwrite).await
        }
        Some(Commands::Init { name }) => {
            Project::init(name.clone())
        }
        Some(Commands::Status { remotes, all }) => {
            let mut proj = Project::new()?;
            proj.status(*remotes, *all).await
        }
        Some(Commands::Stats {  }) => {
            //let proj = Project::new()?;
            //proj.stats()
            Ok(())
        }
        Some(Commands::Update { filename }) => {
            let mut proj = Project::new()?;
            proj.update(filename.as_ref()).await
        }
        Some(Commands::Link { dir, service, key, name, link_only }) => {
            let mut proj = Project::new()?;
            proj.link(dir, service, key, name, link_only).await
        }
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
        Some(Commands::Metadata { title, description }) => {
            let mut proj = Project::new()?;
            proj.set_metadata(title, description)
        },
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }


}
