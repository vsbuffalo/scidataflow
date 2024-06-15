use anyhow::{anyhow, Context, Result};
use csv::{ReaderBuilder, StringRecord};
use dirs;
#[allow(unused_imports)]
use log::{debug, info, trace};
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::env;
use std::fs::{canonicalize, metadata, rename, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::lib::api::figshare::FigShareAPI;
use crate::lib::api::zenodo::ZenodoAPI;
use crate::lib::data::LocalStatusCode;
use crate::lib::data::{DataCollection, DataFile};
use crate::lib::download::Downloads;
use crate::lib::remote::Remote;
use crate::lib::remote::{authenticate_remote, AuthKeys};
use crate::lib::utils::{load_file, pluralize, print_status};
#[allow(unused_imports)]
use crate::{print_info, print_warn};

use super::utils::is_directory;

const MANIFEST: &str = "data_manifest.yml";

pub fn find_manifest(start_dir: Option<&PathBuf>, filename: &str) -> Option<PathBuf> {
    let mut current_dir = match start_dir {
        Some(dir) => dir.to_path_buf(),
        None => env::current_dir().expect("Failed to get current directory"),
    };

    loop {
        let config_path = current_dir.join(filename);

        if metadata(&config_path).is_ok() {
            return Some(config_path);
        }

        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => return None,
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let mut config_path: PathBuf =
        dirs::home_dir().ok_or_else(|| anyhow!("Cannot load home directory!"))?;
    config_path.push(".scidataflow_config");
    Ok(config_path)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub name: String,
    pub email: Option<String>,
    pub affiliation: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Config {
    user: User,
}

// Metadata about *local* project
//
// The idea of this is to extract the parts of the metadata
// that Remote.remote_init() can access, so we can pass
// a single object to Remote.remote_init(). E.g. includes
// User and DataCollectionMetadata.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct LocalMetadata {
    pub author_name: Option<String>,
    pub email: Option<String>,
    pub affiliation: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

impl LocalMetadata {
    pub fn from_project(project: &Project) -> Self {
        LocalMetadata {
            author_name: Some(project.config.user.name.clone()),
            email: project.config.user.email.clone(),
            affiliation: project.config.user.affiliation.clone(),
            title: project.data.metadata.title.clone(),
            description: project.data.metadata.description.clone(),
        }
    }
}

pub struct Project {
    pub manifest: PathBuf,
    pub data: DataCollection,
    pub config: Config,
}

impl Project {
    fn get_manifest() -> Result<PathBuf> {
        find_manifest(None, MANIFEST).ok_or(anyhow!("SciDataFlow not initialized."))
    }

    pub fn load_config() -> Result<Config> {
        let config_path = config_path()?;
        let mut file = File::open(&config_path).map_err(|_| {
            anyhow!(
                "No SciDataFlow config found at \
                                 {:?}. Please set with sdf config --name <NAME> \
                                 [--email <EMAIL> --affiliation <AFFILIATION>]",
                &config_path
            )
        })?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: Config = serde_yaml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save_config(config: Config) -> Result<()> {
        let config_path = config_path()?;
        let serialized_config = serde_yaml::to_string(&config)?;
        std::fs::write(config_path, serialized_config)
            .with_context(|| "Failed to write the configuration to file")?;
        Ok(())
    }

    pub fn new() -> Result<Self> {
        let manifest = Project::get_manifest().context("Failed to get the manifest")?;
        info!("manifest: {:?}", manifest);
        let data = Project::load(&manifest).context("Failed to load data from the manifest")?;
        let config = Project::load_config().context("Failed to load the project configuration")?;
        let proj = Project {
            manifest,
            data,
            config,
        };
        Ok(proj)
    }

    fn get_parent_dir(file: &Path) -> String {
        file.parent()
            .and_then(|path| path.file_name())
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .unwrap_or_else(|| panic!("invalid project location: is it in root?"))
    }

    // This tries to figure out a good default name to use, e.g. for
    // remote titles or names.
    //
    // The precedence is local metadata in manifest > project directory
    pub fn name(&self) -> String {
        if let Some(t) = &self.data.metadata.title {
            return t.to_string();
        }
        Project::get_parent_dir(&self.manifest)
    }

    pub fn init(name: Option<String>) -> Result<()> {
        // the new manifest should be in the present directory
        let manifest: PathBuf = PathBuf::from(MANIFEST);
        if manifest.exists() {
            return Err(anyhow!(
                "Project already initialized. Manifest file already exists."
            ));
        } else {
            // TODO could pass metadata parameters here
            let mut data = DataCollection::new();
            if let Some(name) = name {
                data.metadata.title = Some(name);
            }
            let config = Project::load_config()?;
            let proj = Project {
                manifest,
                data,
                config,
            };
            // save to create the manifest
            proj.save()?;
        }
        Ok(())
    }

    // TODO could add support for other metadata here
    pub fn set_metadata(
        &mut self,
        title: &Option<String>,
        description: &Option<String>,
    ) -> Result<()> {
        if let Some(new_title) = title {
            self.data.metadata.title = Some(new_title.to_string());
        }
        if let Some(new_description) = description {
            self.data.metadata.description = Some(new_description.to_string());
        }
        self.save()
    }

    pub fn set_config(
        name: &Option<String>,
        email: &Option<String>,
        affiliation: &Option<String>,
    ) -> Result<()> {
        let mut config = Project::load_config().unwrap_or_else(|_| Config {
            user: User {
                name: "".to_string(),
                email: None,
                affiliation: None,
            },
        });
        info!("read config: {:?}", config);
        if let Some(new_name) = name {
            config.user.name = new_name.to_string();
        }
        if let Some(new_email) = email {
            config.user.email = Some(new_email.to_string());
        }
        if let Some(new_affiliation) = affiliation {
            config.user.affiliation = Some(new_affiliation.to_string());
        }
        if config.user.name.is_empty() {
            return Err(anyhow!("Config 'name' not set, and cannot be empty."));
        }
        Project::save_config(config)?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        // Serialize the data
        let serialized_data = serde_yaml::to_string(&self.data)
            .map_err(|err| anyhow::anyhow!("Failed to serialize data manifest: {}", err))?;

        // Create the file
        let mut file = File::create(&self.manifest)
            .map_err(|err| anyhow::anyhow!("Failed to open file '{:?}': {}", self.manifest, err))?;

        // Write the serialized data to the file
        write!(file, "{}", serialized_data)
            .map_err(|err| anyhow::anyhow!("Failed to write data manifest: {}", err))?;

        Ok(())
    }

    fn load(manifest: &PathBuf) -> Result<DataCollection> {
        let contents = load_file(manifest);

        if contents.trim().is_empty() {
            // empty manifest, just create a new one
            return Err(anyhow!(
                "No 'data_manifest.yml' found, has sdf init been run?"
            ));
        }

        let data = serde_yaml::from_str(&contents)?;
        Ok(data)
    }

    /// Get the absolute path context of the current project.
    pub fn path_context(&self) -> PathBuf {
        let path = self.manifest.parent().unwrap().to_path_buf();
        debug!("path_context = {:?}", path);
        path
    }

    pub fn resolve_path(&self, path: &String) -> PathBuf {
        let full_path = self.path_context().join(path);
        let resolved_path = canonicalize(full_path).unwrap();
        debug!("resolved_path = {:?}", resolved_path);
        resolved_path
    }

    pub fn relative_path(&self, path: &Path) -> Result<PathBuf> {
        let absolute_path = canonicalize(path).context(format!(
            "Failed to canonicalize path '{}'.",
            path.to_string_lossy()
        ))?;
        //ensure_directory(&absolute_path)?;
        let path_context = canonicalize(self.path_context()).context(format!(
            "Failed to canonicalize path '{}'.",
            path.to_string_lossy()
        ))?;

        // Compute relative path directly using strip_prefix
        match absolute_path.strip_prefix(&path_context) {
            Ok(rel_path) => Ok(rel_path.to_path_buf()),
            Err(_) => Err(anyhow::anyhow!("Failed to compute relative path")),
        }
    }

    pub fn relative_path_string(&self, path: &Path) -> Result<String> {
        if !path.exists() {
            Err(anyhow!("Path '{}' does not exist.", path.to_string_lossy()))
        } else {
            Ok(self.relative_path(path)?.to_string_lossy().to_string())
        }
    }

    pub async fn remove(&mut self, files: &Vec<String>) -> Result<()> {
        let mut num_removed = 0;
        for filename in files {
            info!("Removing file '{}'.", filename);
            let filepath = self.relative_path_string(Path::new(filename))?;
            let removed = self.data.remove(&filepath).await;
            num_removed += removed as i32;
        }
        println!("Removed {}.", pluralize(num_removed as u64, "file"));
        self.save()
    }

    pub async fn status(&mut self, include_remotes: bool, all: bool) -> Result<()> {
        // if include_remotes (e.g. --remotes) is set, we need to merge
        // in the remotes, so we authenticate first and then get them.
        let path_context = &canonicalize(self.path_context())?;
        let status_rows = self.data.status(path_context, include_remotes).await?;
        //let remotes: Option<_> = include_remotes.then(|| &self.data.remotes);
        print_status(status_rows, Some(&self.data.remotes), all);
        Ok(())
    }

    // TODO
    pub async fn is_clean(&self) -> Result<bool> {
        for data_file in self.data.files.values() {
            let status = data_file.status(&self.path_context()).await?;
            if status != LocalStatusCode::Current {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /*
       pub fn stats(&self) -> Result<()> {
       let mut rows: Vec<StatusEntry> = Vec::new();
       for (key, data_file) in self.data.files.iter() {
       let size = format_bytes(data_file.get_size(&self.path_context())?);
       let cols = vec![key.clone(), size];
    // TODO use different more general struct?
    // Or print_fixed_width should be a trait?
    let entry = StatusEntry {
    local_status: LocalStatusCode::Invalid,
    remote_status: RemoteStatusCode::NotExists,
    tracked: Some(false),
    remote_service: None,
    cols: Some(cols) };
    rows.push(entry);
    }
    print_status(rows, None);
    Ok(())
    } */

    pub async fn add(&mut self, files: &Vec<String>) -> Result<()> {
        let mut num_added = 0;
        for filepath in files {
            let filename = self.relative_path_string(Path::new(&filepath.clone()))?;
            let data_file = DataFile::new(filename.clone(), None, &self.path_context()).await?;
            info!("Adding file '{}'.", filename);
            self.data.register(data_file)?;
            num_added += 1;
        }
        println!("Added {}.", pluralize(num_added as u64, "file"));
        self.save()
    }

    pub async fn update(&mut self, files: Option<&Vec<String>>) -> Result<()> {
        let path_context = self.path_context();
        let mut num_updated = 0;

        let filepaths: Result<Vec<String>> = match files {
            None => Ok(self.data.files.keys().cloned().collect::<Vec<String>>()),
            Some(file_list) => file_list
                .iter()
                .map(|f| {
                    Ok(self
                        .relative_path(Path::new(&f))?
                        .to_string_lossy()
                        .to_string())
                })
                .collect(),
        };

        let filepaths = filepaths?; // Use ? here to propagate any errors

        for filepath in filepaths {
            match self.data.update(Some(&filepath), &path_context).await {
                Ok(_) => {
                    info!("Updated file '{}'.", filepath);
                    num_updated += 1;
                }
                Err(e) => {
                    return Err(anyhow!("Failed to update file '{}': {}", filepath, e));
                }
            }
        }
        println!("Updated {}.", pluralize(num_updated as u64, "file"));
        self.save()
    }

    pub async fn link(
        &mut self,
        dir: &str,
        service: &str,
        key: &str,
        name: &Option<String>,
        link_only: &bool,
    ) -> Result<()> {
        // (0) get the relative directory path
        let dir = self.relative_path_string(Path::new(dir))?;

        // (1) save the auth key to home dir
        let mut auth_keys = AuthKeys::new();
        auth_keys.add(service, key);

        // (2) create a new remote, with a name
        // Associate a project (either by creating it, or finding it on FigShare)
        let name = if let Some(n) = name {
            n.to_string()
        } else {
            self.name()
        };

        let service = service.to_lowercase();
        let mut remote = match service.as_str() {
            "figshare" => Ok(Remote::FigShareAPI(FigShareAPI::new(&name, None)?)),
            "zenodo" => Ok(Remote::ZenodoAPI(ZenodoAPI::new(&name, None)?)),
            _ => Err(anyhow!("Service '{}' is not supported!", service)),
        }?;

        // (3) authenticate remote
        authenticate_remote(&mut remote)?;

        // (4) validate this a proper remote directory (this is
        // also done in register_remote() for caution,
        // but we also want do it here to prevent the situation
        // where self.data.register_remote() fails, but remote_init()
        // is already done.
        self.data.validate_remote_directory(&dir)?;

        // (5) initialize the remote (e.g. for FigShare, this
        // checks that the article doesn't exist (error if it
        // does), creates it, and sets the FigShare.article_id
        // once it is assigned by the remote).
        // Note: we pass the Project to remote_init
        let local_metadata = LocalMetadata::from_project(self);
        remote.remote_init(local_metadata, *link_only).await?;

        // (6) register the remote in the manifest
        self.data.register_remote(&dir, remote)?;
        self.save()
    }

    pub async fn ls(&mut self) -> Result<()> {
        let all_remote_files = self.data.merge(true).await?;
        for (directory, remote_files) in all_remote_files.iter() {
            println!("Remote: {}", directory);
            for file in remote_files.values() {
                println!(" - {:?}", file);
            }
        }
        Ok(())
    }

    // Move a file within the project.
    //
    // Note: file moving is done within relatively higher project-level API.
    // The reason why is that we need to access Project::relative_path_string() for
    // both the source *and* destination; the latter does not exist until after the file
    // has been successfully moved. So the updating is all done on the DataFile
    // directly, since lower interfaces cannot access the relative path.
    pub async fn mv(&mut self, source: &str, destination: &str) -> Result<()> {
        let source_path = Path::new(source);
        let source_path_str = self.relative_path_string(source_path)?;
        if let Some(file) = self.data.files.remove(&source_path_str) {
            let mut destination_path = PathBuf::from(destination);

            if is_directory(&destination_path) {
                // if destination is a directory, append the file name from
                // the source path to mimic unix mv
                if let Some(file_name) = source_path.file_name() {
                    destination_path = destination_path.join(file_name);
                }
            }

            // move the actual file
            rename(source, &destination_path).context("Error encountered when moving file.")?;

            // update the relative path
            let relative_destination = self.relative_path_string(destination_path.as_path())?;

            // modify the DataFile
            let mut new_file = file.clone();
            new_file.path = relative_destination;

            // insert it back into the map with the new key
            self.data.files.insert(destination.to_string(), new_file);

            self.save()
        } else {
            Err(anyhow!(
                "Cannot move file '{}' with 'sdf mv' since it is not in the manifest.",
                source
            ))
        }
    }

    pub async fn get(&mut self, url: &str, filename: Option<&str>, overwrite: bool) -> Result<()> {
        let mut downloads = Downloads::new();
        let download = downloads.add(url.to_string(), filename, overwrite)?;
        if let Some(dl) = download {
            let filepath = dl.filename.clone();

            // get the file
            downloads
                .retrieve(Some("Downloaded '{}'."), None, false)
                .await?;

            // convert to relative path (based on where we are)
            let filepath = self.relative_path_string(Path::new(&filepath))?;

            // TODO: should compare MD5s!
            if !self.data.contains(&filepath).await? {
                let data_file =
                    DataFile::new(filepath.clone(), Some(url), &self.path_context()).await?;

                // Note: we do not use Project::add() since this works off strings.
                // and we need to pass the URL, etc.
                self.data.register(data_file)?;
                self.save()?;
            } else {
                println!(
                    "File '{}' already existed in \
                    the manifest, so it was not added.",
                    &filepath
                );
            }
            Ok(())
        } else {
            Err(anyhow!(
                "The file at '{}' was not downloaded because it would overwrite a file.\n\
                    Use 'sdf get <URL> --ovewrite' to overwrite it.",
                url
            ))
        }
    }

    pub async fn bulk(
        &mut self,
        filename: &str,
        column: Option<u64>,
        header: bool,
        overwrite: bool,
    ) -> Result<()> {
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(std::ffi::OsStr::to_str);

        let delimiter = match extension {
            Some("csv") => b',',
            Some("tsv") => b'\t',
            _ => return Err(anyhow!("Unsupported file type: {:?}", extension)),
        };

        let file = File::open(filename)?;
        let mut reader = ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(header)
            .from_reader(file);

        // convert 0-indexed to 1; first column is default
        let column = column.unwrap_or(0) as usize - 1;

        let mut downloads = Downloads::new();
        let mut filepaths = Vec::new();
        let mut urls = Vec::new();
        let mut skipped = Vec::new();
        let mut num_lines = 0;
        for result in reader.records() {
            let record: StringRecord = result?;
            if let Some(url) = record.get(column) {
                num_lines += 1;
                let url = url.to_string();
                let download = downloads.add(url.clone(), None, overwrite)?;
                if let Some(dl) = download {
                    let filepath = dl.filename.clone();
                    filepaths.push(filepath);
                    urls.push(url.clone());
                } else {
                    skipped.push(url.clone());
                }
            }
        }

        // grab all the files
        downloads.retrieve(None, None, false).await?;

        let mut num_added = 0;
        let mut num_already_registered = 0;
        for (filepath, url) in filepaths.iter().zip(urls.iter()) {
            let rel_file_path = self.relative_path_string(Path::new(&filepath))?;
            if !self.data.contains(&rel_file_path).await? {
                let data_file =
                    DataFile::new(rel_file_path.clone(), Some(url), &self.path_context()).await?;
                self.data.register(data_file)?;
                num_added += 1;
            } else {
                num_already_registered += 1;
            }
        }
        let num_skipped = skipped.len();
        println!(
            "{} URLs found in '{}.'\n\
            {} files were downloaded, {} added to manifest ({} were already registered).\n\
            {} files were skipped because they existed (and --overwrite was no specified).",
            num_lines,
            filename,
            urls.len(),
            num_added,
            num_already_registered,
            num_skipped
        );
        self.save()?;
        Ok(())
    }

    pub fn untrack(&mut self, filepath: &String) -> Result<()> {
        let filepath = self.relative_path_string(Path::new(filepath))?;
        self.data.untrack_file(&filepath)?;
        self.save()
    }

    pub fn track(&mut self, filepath: &String) -> Result<()> {
        let filepath = self.relative_path_string(Path::new(filepath))?;
        self.data.track_file(&filepath, &self.path_context())?;
        self.save()
    }

    pub async fn pull(
        &mut self,
        overwrite: bool,
        url: bool,
        all: bool,
        limit: &Option<PathBuf>,
    ) -> Result<()> {
        let path_context = self.path_context();
        if all {
            self.data.pull_urls(&path_context, overwrite, limit).await?;
            return self.data.pull(&path_context, overwrite, limit).await;
        }
        if url {
            return self.data.pull_urls(&path_context, overwrite, limit).await;
        }
        self.data.pull(&path_context, overwrite, limit).await
    }

    pub async fn push(&mut self, overwrite: bool) -> Result<()> {
        self.data.push(&self.path_context(), overwrite).await
    }
}
