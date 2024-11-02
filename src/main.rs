use std::path::Path;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
#[allow(unused_imports)]
use log::{debug, info, trace};
use scidataflow::lib::assets::GitHubRepo;
use scidataflow::lib::download::Downloads;
use tokio::runtime::Builder;

use scidataflow::lib::project::Project;
use scidataflow::logging_setup::setup;

pub mod logging_setup;

const SDF_ASSET_URL: &str = "https://github.com/scidataflow-assets";

const INFO: &str = "\
SciDataFlow: Manage and Share Scientific Data
usage: sdf [--help] <subcommand>

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

  Pull in data from the URLs in the manifest only (you may want --overwrite)
  $ sdf pull --url

  Pull in data from URLs and remotes
  $ sdf pull --all
 
  Push data to a remote (you may want --overwrite):
  $ sdf push

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
#[clap(name = "sdf")]
#[clap(about = INFO)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a data file to the manifest.
    Add {
        /// the file to begin tracking.
        #[arg(required = true)]
        filenames: Vec<String>,
    },
    /// Set local system-wide metadata (e.g. your name, email, etc.), which
    /// can be propagated to some APIs.
    Config {
        /// Your name.
        #[arg(long)]
        name: Option<String>,
        // Your email.
        #[arg(long)]
        email: Option<String>,
        // Your affiliation.
        #[arg(long)]
        affiliation: Option<String>,
    },
    /// Initialize a new project.
    Init {
        /// Project name (default: the name of the directory).
        #[arg(long)]
        name: Option<String>,
    },
    /// Download a file from a URL.
    Get {
        /// Download filename (default: based on URL).
        url: String,
        #[arg(long)]
        name: Option<String>,
        /// Overwrite local files if they exit.
        #[arg(long)]
        overwrite: bool,
    },
    /// Download a bunch of files from links stored in a file.
    Bulk {
        /// A TSV or CSV file containing a column of URLs. Type inferred from suffix.
        filename: String,
        /// Which column contains links (default: first).
        #[arg(long)]
        column: Option<u64>,
        /// The TSV or CSV starts with a header (i.e. skip first line).
        #[arg(long)]
        header: bool,
        /// Overwrite local files if they exit.
        #[arg(long)]
        overwrite: bool,
    },
    /// Show status of data.
    Status {
        /// Show remotes status (requires network).
        #[arg(long)]
        remotes: bool,

        /// Show statuses of all files, including those on remote(s) but not in the manifest.
        #[arg(long)]
        all: bool,
    },
    /// Show file size statistics.
    Stats {},
    /// Update MD5s
    Update {
        /// Which file to update (if not set, all tracked files are update).
        #[arg(required = false)]
        filenames: Vec<String>,
        /// Update all files presently registered in the manifest.
        #[arg(long)]
        all: bool,
    },
    /// Remove a file from the manifest
    Rm {
        /// Which file(s) to remove from the manifest (these are not deleted).
        #[arg(required = true)]
        filenames: Vec<String>,
    },
    /// Retrieve a SciDataFlow Asset
    Asset {
        /// A GitHub link
        #[arg(long)]
        github: Option<String>,
        /// A URL to a data_manifest.yml file
        #[arg(long)]
        url: Option<String>,
        /// A SciDataFlow Asset name
        asset: Option<String>,
    },
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
        #[arg(long)]
        name: Option<String>,

        /// Don't initialize remote, only add to manifest. This will retrieve
        /// the remote information (i.e. the FigShare Article ID or Zenodo
        /// Depository ID) to add to the manifest. Requires network.
        #[arg(long)]
        link_only: bool,
    },
    /// No longer keep track of this file on the remote.
    Untrack {
        /// The file to untrack with remote.
        filename: String,
    },
    /// Keep track of this file on the remote.
    Track {
        /// The file to track with remote.
        filename: String,
    },
    /// Move or rename a file on the file system and in the manifest.
    Mv { source: String, destination: String },
    /// Push all tracked files to remote.
    Push {
        /// Overwrite remote files if they exit.
        #[arg(long)]
        overwrite: bool,
    },
    /// Pull in all tracked files from the remote. If --urls is set,
    /// this will (re)-download all files (tracked or not) in that manifest
    /// from their URLs.
    ///
    /// Note that if --overwrite is set, this will append the suffix '.tmp'
    /// to each file that will be replaced, and those files will be removed
    /// after the download is successful. While safer, this does temporarily
    /// increase disk usage.
    Pull {
        /// Overwrite local files if they exit.
        #[arg(long)]
        overwrite: bool,

        /// Pull in files from the URLs, not remotes.
        #[arg(long)]
        urls: bool,

        /// Pull in files from remotes and URLs.
        #[arg(long)]
        all: bool,
        // multiple optional directories
        //directories: Vec<PathBuf>,
    },
    /// Change the project metadata.
    Metadata {
        /// The project name.
        #[arg(long)]
        title: Option<String>,
        // A description of the project.
        #[arg(long)]
        description: Option<String>,
    },
}

pub fn print_errors(response: Result<()>) {
    match response {
        Ok(_) => {}
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
        Some(Commands::Config {
            name,
            email,
            affiliation,
        }) => Project::set_config(name, email, affiliation),
        Some(Commands::Get {
            url,
            name,
            overwrite,
        }) => {
            let mut proj = Project::new()?;
            proj.get(url, name.as_deref(), *overwrite).await
        }
        Some(Commands::Bulk {
            filename,
            column,
            header,
            overwrite,
        }) => {
            let mut proj = Project::new()?;
            proj.bulk(filename, *column, *header, *overwrite).await
        }
        Some(Commands::Init { name }) => Project::init(name.clone()),
        Some(Commands::Status { remotes, all }) => {
            let mut proj = Project::new()?;
            proj.status(*remotes, *all).await
        }
        Some(Commands::Stats {}) => {
            //let proj = Project::new()?;
            //proj.stats()
            Ok(())
        }
        Some(Commands::Rm { filenames }) => {
            let mut proj = Project::new()?;
            proj.remove(filenames).await
        }
        Some(Commands::Update { filenames, all }) => {
            let mut proj = Project::new()?;
            if !*all && filenames.is_empty() {
                return Err(anyhow!("Specify --all or one or more file to update."));
            }
            let filepaths = if *all { None } else { Some(filenames) };
            proj.update(filepaths).await
        }
        Some(Commands::Link {
            dir,
            service,
            key,
            name,
            link_only,
        }) => {
            let mut proj = Project::new()?;
            proj.link(dir, service, key, name, link_only).await
        }
        Some(Commands::Track { filename }) => {
            let mut proj = Project::new()?;
            proj.track(filename)
        }
        Some(Commands::Untrack { filename }) => {
            let mut proj = Project::new()?;
            proj.untrack(filename)
        }
        Some(Commands::Mv {
            source,
            destination,
        }) => {
            let mut proj = Project::new()?;
            proj.mv(source, destination).await
        }
        Some(Commands::Push { overwrite }) => {
            let mut proj = Project::new()?;
            proj.push(*overwrite).await
        }
        Some(Commands::Pull {
            overwrite,
            urls,
            all,
        }) => {
            let mut proj = Project::new()?;
            proj.pull(*overwrite, *urls, *all).await
        }
        Some(Commands::Metadata { title, description }) => {
            let mut proj = Project::new()?;
            proj.set_metadata(title, description)
        }
        Some(Commands::Asset { github, url, asset }) => {
            if Path::new("data_manifest.yml").exists() {
                return Err(anyhow!("data_manifest.yml already exists in the current directory; delete it manually first to use sdf asset."));
            }
            let msg = "Set either --github, --url, or specify an SciDataFlow Asset name.";
            let url = match (github, url, asset) {
                (Some(gh), None, None) => {
                    let gh = GitHubRepo::new(gh)
                        .map_err(|e| anyhow!("GitHubRepo initialization failed: {}", e))?;
                    gh.url("data_manifest.yml")
                }
                (None, None, Some(asset)) => {
                    let url = format!("{}/{}", SDF_ASSET_URL, asset);
                    let gh = GitHubRepo::new(&url)
                        .expect("Internal Error: invalid Asset URL; please report.");
                    gh.url("data_manifest.yml")
                }
                (None, Some(url), None) => url.to_string(),
                _ => return Err(anyhow!(msg)),
            };
            let mut downloads = Downloads::new();
            downloads.add(url.clone(), None, false)?;
            downloads.retrieve(None, None, false).await?;
            Ok(())
        }
        None => {
            println!("{}\n", INFO);
            std::process::exit(1);
        }
    }
}
