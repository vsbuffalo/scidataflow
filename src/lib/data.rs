use std::path::{PathBuf,Path};
use anyhow::{anyhow,Result};
use std::fs::{metadata};
use serde_derive::{Serialize,Deserialize};
use serde::ser::SerializeMap;
use serde;
#[allow(unused_imports)]
use log::{info, trace, debug};
use chrono::prelude::*;
use std::collections::{HashMap,BTreeMap};
use futures::future::join_all;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::fs;
use trauma::downloader::{DownloaderBuilder,StyleOptions,ProgressBarOpts};
use std::time::Duration;
use std::thread;
use indicatif::{ProgressBar, ProgressStyle};
use colored::*;

use crate::{print_warn,print_info};
use crate::lib::utils::{format_mod_time,compute_md5, md5_status,pluralize};
use crate::lib::remote::{authenticate_remote,Remote,RemoteFile,RemoteStatusCode};

// The status of a local data file, *conditioned* on it being in the manifest.
#[derive(Debug,PartialEq,Clone)]
pub enum LocalStatusCode {
    Current,     // The MD5s between the file and manifest agree
    Modified,    // The MD5s disagree
    Deleted,     // The file is in the manifest but not file system
    Invalid      // Invalid state
}

#[derive(Debug,Clone)]
pub struct StatusEntry {
    pub name: String,
    pub local_status: Option<LocalStatusCode>,
    pub remote_status: Option<RemoteStatusCode>,
    pub tracked: Option<bool>,
    pub remote_service: Option<String>,
    pub local_md5: Option<String>,
    pub remote_md5: Option<String>,
    pub manifest_md5: Option<String>,
    pub local_mod_time: Option<DateTime<Utc>>
}

impl StatusEntry {
    fn local_md5_column(&self, abbrev: Option<i32>) -> Result<String> {
        Ok(md5_status(self.local_md5.as_ref(), self.manifest_md5.as_ref(), abbrev))
    }
    fn remote_md5_column(&self, abbrev: Option<i32>) -> Result<String> {
        Ok(md5_status(self.remote_md5.as_ref(), self.manifest_md5.as_ref(), abbrev))
    }
    // StatusEntry.remote_status can be set to None; if so the remote status
    // columns will no be displayed.
    fn include_remotes(&self) -> bool {
        self.remote_service.is_some()
    }
    pub fn color(&self, line: String) -> String {
        // color is polymorphic on whether remote_status is None.
        let tracked = self.tracked;
        let local_status = &self.local_status;
        let remote_status = &self.remote_status;
        match (tracked, local_status, remote_status) {
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Current)) => line.green().to_string(),
            (Some(true), Some(LocalStatusCode::Current), None) => line.green().to_string(),
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::NotExists)) => line.green().to_string(),
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::NotExists)) => line.green().to_string(),
            // not tracked, but on remote
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Current)) => line.cyan().to_string(),
            // not tracked, not on remote
            (Some(false), Some(LocalStatusCode::Current), None) => line.green().to_string(),
            // not tracked, no remote but everything is current 
            (None, Some(LocalStatusCode::Current), None) => line.green().to_string(),

            (Some(true), Some(LocalStatusCode::Modified), _)  => line.red().to_string(),
            (Some(false), Some(LocalStatusCode::Modified), _)  => line.red().to_string(),
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Different))  => line.yellow().to_string(),
            // untracked, but exists on remote -- invalid
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Different))  => line.cyan().to_string(),
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Exists))  => line.cyan().to_string(),
            _ => {
                //println!("{:?}: {:?}, {:?}, {:?}", self.name, tracked, local_status, remote_status);
                line.cyan().to_string()
            }
        }
    }
    pub fn columns(&self, abbrev: Option<i32>) -> Result<Vec<String>> {
        let local_status = &self.local_status;

        let md5_string = self.local_md5_column(abbrev)?;

        let mod_time_pretty = self.local_mod_time.map(format_mod_time).unwrap_or_default();

        // append a local status message column
        let local_status_msg = match local_status {
            Some(LocalStatusCode::Current) => "current",
            Some(LocalStatusCode::Modified) => "changed",
            Some(LocalStatusCode::Deleted) => "deleted",
            Some(LocalStatusCode::Invalid) => "invalid",
            _ => "no file"
        };

        let tracked = match (self.include_remotes(), self.tracked) {
            (false, _) => "".to_string(),
            (true, Some(true)) => ", tracked".to_string(),
            (true, Some(false)) => ", untracked".to_string(),
            (_, _) => return Err(anyhow!("Invalid tracking state"))
        };
        let mut columns = vec![
            self.name.clone(),
            format!("{}{}", local_status_msg, tracked),
            md5_string,
            mod_time_pretty,
        ];

        if self.include_remotes() {
            let remote_status_msg = match &self.remote_status {
                Some(RemoteStatusCode::Current) => "identical remote".to_string(),
                Some(RemoteStatusCode::MessyLocal) => "messy local".to_string(),
                Some(RemoteStatusCode::Different) => {
                    format!("different remote version ({:})", self.remote_md5_column(abbrev)?)
                },
                Some(RemoteStatusCode::NotExists) => "not on remote".to_string(),
                Some(RemoteStatusCode::NoLocal) => "unknown (messy remote)".to_string(),
                Some(RemoteStatusCode::Exists) => "exists, no remote MD5".to_string(),
                Some(RemoteStatusCode::DeletedLocal) => "exists on remote".to_string(),
                _ => "invalid".to_string()
            };
            columns.push(remote_status_msg.to_string());
        }

        Ok(columns)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataFile {
    pub path: String,
    pub tracked: bool,
    pub md5: String,
    pub size: u64,
    //modified: Option<DateTime<Utc>>,
}

// A merged DataFile and RemoteFile
// 
// remote_service: Some(String) remote name if this file's directory 
// is linked to a remote. None if there is no remote. None distinguishes 
// the important cases when remote = NotExists (there is no remote
// file) due to there not being a remote tracking, and remote = NotExists
// due to the remote being configured, but the file not existing (e.g.
// not found in the merge).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergedFile {
    pub local: Option<DataFile>,
    pub remote: Option<RemoteFile>,
    pub remote_service: Option<String>
}


impl MergedFile {
    pub fn new(data_file: &DataFile, remote_file: &RemoteFile, remote_service: Option<String>) -> Result<MergedFile> {
        Ok(MergedFile {
            local: Some(data_file.clone()),
            remote: Some(remote_file.clone()),
            remote_service
        })
    }

    pub fn name(&self) -> Result<String> {
        match (&self.local, &self.remote) {
            (Some(local), Some(remote)) => {
                let local_name = local.basename()?;
                if local_name == remote.name {
                    Ok(local_name)
                } else {
                    Err(anyhow!("Local and remote names do not match."))
                }
            },
            (Some(local), None) => Ok(local.basename()?),
            (None, Some(remote)) => Ok(remote.name.clone()),
            (None, None) => Err(anyhow!("Invalid state: both local and remote are None.")),
        }
    }

    pub fn can_download(&self) -> bool {
        self.local.is_some() && self.remote.is_some()
    }

    pub fn has_remote(&self) -> Result<bool> {
        Ok(self.remote.is_some())
    }

    pub fn is_tracked(&self) -> Option<bool> {
        self.local.as_ref().map(|data_file| data_file.tracked)
    }

    pub fn local_md5(&self, path_context: &Path) -> Option<String> {
        self.local.as_ref()
            .and_then(|local| local.get_md5(path_context).ok())
            .flatten()
    }

    pub fn remote_md5(&self) -> Option<String> {
        self.remote.as_ref()
            .and_then(|remote| remote.get_md5())
    }

    //pub fn local_md5_mismatch(&self, path_context: &PathBuf) -> Option<bool> {
    //}

    pub fn manifest_md5(&self) -> Option<String> {
        self.local.as_ref().map(|local| local.md5.clone())
    }

    pub fn local_remote_md5_mismatch(&self, path_context: &Path) -> Option<bool> {
        let local_md5 = self.local_md5(path_context);
        let remote_md5 = self.remote_md5();
        match (remote_md5, local_md5) {
            (Some(remote), Some(local)) => Some(remote != local),
            _ => None,
        }
    }

    pub fn local_mod_time(&self, path_context: &Path) -> Option<DateTime<Utc>> {
        self.local.as_ref()
            .and_then(|data_file| data_file
                      .get_mod_time(path_context).ok())
    }

    pub fn status(&self, path_context: &Path) -> Result<RemoteStatusCode> {
        //let tracked = self.local.as_ref().map_or(None,|df| Some(df.tracked));

        // local status, None if no local file found
        let local_status = self.local
            .as_ref()
            .and_then(|local| local.status(path_context).ok());
        // TODO fix path_context
        //info!("{:?} local status: {:?} ({:?})", self.name(), local_status, &path_context);

        let md5_mismatch = self.local_remote_md5_mismatch(path_context);
    
        if !self.has_remote().unwrap_or(false) {
            return Ok(RemoteStatusCode::NotExists)
        }

        // MergedFile has a remote, so get the remote status.
        let status = match (&local_status, &md5_mismatch) {
            (None, None) => {
                // no local file (so can't get MD5)
                RemoteStatusCode::NoLocal
            },
            (Some(LocalStatusCode::Current), Some(false)) => {
                RemoteStatusCode::Current
            },
            (Some(LocalStatusCode::Current), Some(true)) => {
                // Will pull with --overwrite. 
                // Will push with --overwrite.
                RemoteStatusCode::Different
            },
            (Some(LocalStatusCode::Current), None) => {
                // We can't compare the MD5s, i.e. because remote 
                // does not support them
                RemoteStatusCode::Exists
            },
            (Some(LocalStatusCode::Modified), _) => {
                // Messy local -- this will prevent syncing!
                // TODO: could compare the MD5s here further
                // and separate out modified local (manifest and remote agree)
                // and messy (manifest out of date)?
                RemoteStatusCode::MessyLocal
            },
            (Some(LocalStatusCode::Deleted), _) => {
                // Local file on file system does not exist,
                // but exists in the manifest. If the file is in 
                // the manifest and tracked a pull would pull it in.
                RemoteStatusCode::DeletedLocal
            },
            (_, _) => RemoteStatusCode::Invalid
        };

        Ok(status)
    }


    // Create a StatusEntry, for printing the status to the user.
    pub async fn status_entry(&self, path_context: &Path, include_remotes: bool) -> Result<StatusEntry> {
        let tracked = self.local.as_ref().map(|df| df.tracked);
        let local_status = self.local
            .as_ref()
            .and_then(|local| local.status(path_context).ok());

        let remote_status = if include_remotes { Some(self.status(path_context)?) } else { None };
        //let remote_status = if self.remote_service.is_some() { Some(self.status(path_context)?) } else { None };
        
        let remote_service = if include_remotes { self.remote_service.clone() } else { None };

        if self.local.is_none() && self.remote.is_none() {
            return Err(anyhow!("Internal error: MergedFile with no RemoteFile and DataFile set. Please report."));
        }

        Ok(StatusEntry {
            name: self.name()?,
            local_status,
            remote_status,
            tracked,
            remote_service,
            local_md5: self.local_md5(path_context),
            remote_md5: self.remote_md5(),
            manifest_md5: self.manifest_md5(),
            local_mod_time: self.local_mod_time(path_context)
        })
    }
}


impl DataFile {
    pub fn new(path: String, path_context: &Path) -> Result<DataFile> {
        let full_path = path_context.join(&path);
        if !full_path.exists() {
            return Err(anyhow!("File '{}' does not exist.", path))
        }
        let md5 = match compute_md5(&full_path)? {
            Some(md5) => md5,
            None => return Err(anyhow!("Could not compute MD5 as file does not exist")),
        };
        let size = metadata(full_path)
            .map_err(|err| anyhow!("Failed to get metadata for file {:?}: {}", path, err))?
            .len();
        Ok(DataFile {
            path,
            tracked: false, 
            md5,
            size,
        })
    }

    pub fn full_path(&self, path_context: &Path) -> Result<PathBuf> {
        Ok(path_context.join(self.path.clone()))
    }

    pub fn basename(&self) -> Result<String> {
        let path = Path::new(&self.path);
        match path.file_name() {
            Some(basename) => Ok(basename.to_string_lossy().to_string()),
            None => Err(anyhow!("could not get basename of '{}'", self.path))
        }
    }

    pub fn directory(&self) -> Result<String> {
        let path = std::path::Path::new(&self.path);
        Ok(path.parent()
           .unwrap_or(path)
           .to_str()
           .unwrap_or("")
           .to_string())
    }

    pub fn get_md5(&self, path_context: &Path) -> Result<Option<String>> {
        compute_md5(&self.full_path(path_context)?)
    }

    pub fn get_mod_time(&self, path_context: &Path) -> Result<DateTime<Utc>> {
        let metadata = fs::metadata(self.full_path(path_context)?)?;
        let mod_time = metadata.modified()?.into();
        Ok(mod_time)
    }

    pub fn get_size(&self, path_context: &Path) -> Result<u64> {
        // use metadata() method to get file metadata and extract size
        let size = metadata(self.full_path(path_context)?)
            .map_err(|err| anyhow!("Failed to get metadata for file {:?}: {}", self.path, err))?
            .len();
        Ok(size)
    }

    pub fn is_alive(&self, path_context: &Path) -> bool {
        path_context.join(&self.path).exists()
    }


    // Returns true if the file does not exist.
    pub fn is_changed(&self, path_context: &Path) -> Result<bool> {
        match self.get_md5(path_context)? {
            Some(new_md5) => Ok(new_md5 != self.md5),
            None => Ok(true),
        }
    }

    pub fn status(&self, path_context: &Path) -> Result<LocalStatusCode> {
        let is_alive = self.is_alive(path_context);
        let is_changed = self.is_changed(path_context)?;
        let local_status = match (is_changed, is_alive) {
            (false, true) => LocalStatusCode::Current,
            (true, true) => LocalStatusCode::Modified,
            (false, false) => LocalStatusCode::Deleted,  // Invalid? (TODO)
            (true, false) => LocalStatusCode::Deleted,
            // incase a line gets dropped above
            #[allow(unreachable_patterns)]
            _ => LocalStatusCode::Invalid,
        };
        Ok(local_status)
    }

    pub fn update(&mut self, path_context: &Path) -> Result<()> {
        self.update_md5(path_context)?;
        self.update_size(path_context)?;
        Ok(())
    }

    pub fn update_size(&mut self, path_context: &Path) -> Result<()> {
        let new_size = self.get_size(path_context)?;
        self.size = new_size;
        Ok(())
    }

    pub fn update_md5(&mut self, path_context: &Path) -> Result<()> {
        let new_md5 = match self.get_md5(path_context)? {
            Some(md5) => md5,
            None => return Err(anyhow!("Cannot update MD5: file does not exist")),
        };
        self.md5 = new_md5;
        Ok(())
    }
    /// Mark the file to track on the remote
    pub fn set_tracked(&mut self) -> Result<()> {
        if self.tracked {
            return Err(anyhow!("file '{}' is already tracked on remote.", self.path))
        }
        self.tracked = true;
        Ok(())
    }
    /// Mark the file to not track on the remote
    pub fn set_untracked(&mut self) -> Result<()> {
        if !self.tracked {
            return Err(anyhow!("file '{}' is already not tracked on remote.", self.path))
        }
        self.tracked = false;
        Ok(())
    }
}

fn ordered_map<K, V, S>(value: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
K: serde::Serialize + Ord,
V: serde::Serialize,
S: serde::ser::Serializer,
{
    let mut ordered: Vec<_> = value.iter().collect();
    ordered.sort_by_key(|a| a.0);

    let mut map = serializer.serialize_map(Some(ordered.len()))?;
    for (k, v) in ordered {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

#[derive(Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct DataCollectionMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
}

/// DataCollection structure for managing the data manifest 
/// and how it talks to the outside world.
#[derive(Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct DataCollection {
    #[serde(serialize_with = "ordered_map")]
    pub files: HashMap<String, DataFile>,
    #[serde(serialize_with = "ordered_map")]
    pub remotes: HashMap<String, Remote>, // key is tracked directory
    pub metadata: DataCollectionMetadata,
}

/// DataCollection methods: these should *only* be for 
/// interacting with the data manifest (including remotes).
impl DataCollection {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            remotes: HashMap::new(),
            metadata: DataCollectionMetadata::default()
        }
    }

    // Authenticate all remotes, if there are any.
    // This appends the token to the right Remote struct.
    pub fn authenticate_remotes(&mut self) -> Result<()> {
        if !self.remotes.is_empty() {
            for remote in self.remotes.values_mut() {
                authenticate_remote(remote)?;
            }
        }
        Ok(())
    }

    pub fn register(&mut self, data_file: DataFile) -> Result<()> {
        let path = data_file.path.clone();
        if let std::collections::hash_map::Entry::Vacant(e) = self.files.entry(path) {
            e.insert(data_file);
            Ok(())
        } else {
            Err(anyhow!("File '{}' is already registered in the data manifest. \
                        If you wish to update the MD5 or metadata, use: sdf update FILE",
                        &data_file.path))
        }
    }

    pub fn update(&mut self, filename: Option<&String>, path_context: &Path) -> Result<()> {
        match filename {
            Some(file) => {
                if let Some(data_file) = self.files.get_mut(file) {
                    data_file.update(path_context)?;
                    debug!("rehashed file {:?}", data_file.path);
                }
            }
            None => {
                // 
                let all_files: Vec<_> = self.files.keys().cloned().collect();
                for file in all_files {
                    if let Some(data_file) = self.files.get_mut(&file) {
                        data_file.update(path_context)?;
                        debug!("rehashed file {:?}", data_file.path);
                    }

                }

            }
        }
        Ok(())
    }


    // Validate the directory as being tracked by a remote, 
    // i.e. no nesting.
    pub fn validate_remote_directory(&self, dir: &String) -> Result<()> {
        let dir_path = Path::new(dir);
        // check if the directory itself is already tracked.
        if self.remotes.contains_key(dir) {
            return Err(anyhow!("Directory '{}' is already tracked in the data manifest. You can manually delete it and re-add.", dir));
        }

        // check if the provided directory is a parent of any directory in remotes.
        for existing_dir in self.remotes.keys() {
            let existing_path = Path::new(existing_dir);
            if dir_path.starts_with(existing_path) {
                return Err(anyhow!("Cannot add '{}' because its subdirectory '{}' is already tracked.", dir, existing_dir));
            }
        }

        // check if any directory in remotes is a parent of the provided directory.
        for existing_dir in self.remotes.keys() {
            let existing_path = Path::new(existing_dir);
            if existing_path.starts_with(dir_path) {
                return Err(anyhow!("Cannot add '{}' because it is a subdirectory of already tracked directory '{}'.", dir, existing_dir));
            }
        }
        Ok(())
    }

    pub fn get_this_files_remote(&self, data_file: &DataFile) -> Result<Option<String>> {
        let path = data_file.directory()?;
        let res: Vec<String> = self.remotes.iter()
            .filter(|(r, _v)| PathBuf::from(&path).starts_with(r))
            .map(|(_r, v)| v.name().to_string())
            .collect();

        match res.len() {
            0 => Ok(None),
            1 => Ok(Some(res[0].clone())),
            _ => Err(anyhow!("Invalid state: too many remotes found.")),
        }
    }

    // Register the remote
    //
    // This can overwrite existing entries.
    pub fn register_remote(&mut self, dir: &String, remote: Remote) -> Result<()> {
        self.validate_remote_directory(dir)?;
        self.remotes.insert(dir.to_string(), remote);
        Ok(())
    }

    pub fn get_remote(&mut self, dir: &String) -> Result<&Remote> {
        match self.remotes.get(dir) {
            Some(remote) => Ok(remote),
            None => Err(anyhow!("No such remote")),
        }
    }
    pub fn track_file(&mut self, filepath: &String) -> Result<()> {
        debug!("complete files: {:?}", self.files);
        let data_file = self.files.get_mut(filepath);

        // extract the directory from the filepath
        let dir_path = Path::new(filepath).parent()
            .ok_or_else(|| anyhow!("Failed to get directory for file '{}'", filepath))?;

        // check if the directory exists in self.remotes
        if !self.remotes.contains_key(dir_path.to_str().unwrap_or_default()) {
            return Err(anyhow!("Directory '{}' is not registered in remotes.", dir_path.display()));
        }

        match data_file {
            None => Err(anyhow!("Data file '{}' is not in the data manifest. Add it first using:\n \
                                $ sdf track {}\n", filepath, filepath)),
            Some(data_file) => data_file.set_tracked()
        }
    }
    pub fn untrack_file(&mut self, filepath: &String) -> Result<()> {
        let data_file = self.files.get_mut(filepath);
        match data_file {
            None => Err(anyhow!("Cannot untrack data file '{}' since it was never added to\
                                the data manifest.", filepath)),
            Some(data_file) => data_file.set_untracked()
        }
    }

    // Get local DataFiles by directory
    pub fn get_files_by_directory(&self) -> Result<HashMap<String,Vec<&DataFile>>> {
        let mut dir_map: HashMap<String, Vec<&DataFile>> = HashMap::new();
        for (path, data_file) in self.files.iter() {
            let path = Path::new(&path);
            if let Some(parent_path) = path.parent() {
                let parent_dir = parent_path.to_string_lossy().into_owned();
                dir_map.entry(parent_dir).or_default().push(data_file);
            }
        }
        Ok(dir_map)
    }

    // Fetch all remote files.
    //
    // (remote service, path) -> { filename -> RemoteFile, ... }
    pub async fn fetch(&mut self) -> Result<HashMap<(String, String), HashMap<String, RemoteFile>>> {
        self.authenticate_remotes()?;

        let mut all_remote_files = HashMap::new();
        let pb = ProgressBar::new(self.remotes.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
                     .progress_chars("=> ")
                     .template("{spinner:.green} [{bar:40.green/white}] {pos:>}/{len} ({percent}%) eta {eta_precise:.green} {msg}")?
                    );
        pb.set_message("Fetching remote files...");

        // Convert remotes into Futures, so that they can be awaited in parallel
        let fetch_futures: Vec<_> = self.remotes.iter().map(|(path, remote)| {
            let remote_name = remote.name().to_string();
            let path_clone = path.clone();
            async move {
                let remote_files = remote.get_files_hashmap().await?;
                Ok(((remote_name, path_clone), remote_files))
            }
        }).collect();

        let results = join_all(fetch_futures).await;

        for result in results {
            match result {
                Ok((key, value)) => {
                    pb.set_message(format!("Fetching remote files...   {} done.", key.0));
                    all_remote_files.insert(key, value);
                    pb.inc(1);
                },
                Err(e) => return Err(e), // Handle errors as needed
            }
        }

        pb.finish_with_message("Fetching completed.");
        Ok(all_remote_files)
    }
    // Merge all local and remote files.
    //
    // Use a fetch to get all remote files (as RemoteFile), and merge these 
    // in with the local data files (DataFile) into a MergedFile struct.
    // Missing remote/local files are None.
    // 
    // Returns: Result with HashMap of directory -> { File -> MergedFile, ... } 
    pub async fn merge(&mut self, include_remotes: bool) -> Result<HashMap<String, HashMap<String, MergedFile>>> {
        // directory -> {(filename -> MergedFile), ...}
        let mut result: HashMap<String, HashMap<String, MergedFile>> = HashMap::new();


        // Initialize the result with local files
        // TODO: we need to fix remote_service here, for the
        // case where we have a local file in a tracked directory
        // but it won't merge with a remote file later on.
        for (name, local_file) in &self.files {
            let remote_service = self.get_this_files_remote(local_file)?;
            //info!("local_file: {:?}", local_file);
            let dir = local_file.directory()?;
            result.entry(dir).or_insert_with(HashMap::new)
                .insert(name.clone(),
                MergedFile { local: Some(local_file.clone()), remote: None, remote_service  });
        }

        if !include_remotes {
            return Ok(result)
        }

        // iterate through each remote and retrieve remote files
        let all_remote_files = self.fetch().await?;
        for ((remote_service, tracked_dir), remote_files) in all_remote_files.iter() {
            // merge remote files with local files
            for (name, remote_file) in remote_files {
                // try to get the tracked directory; it doesn't exist make it
                let path_key = PathBuf::from(tracked_dir).join(name).to_str().unwrap().to_string();
                if let Some(merged_file) = result.entry(tracked_dir.clone())
                    .or_insert_with(HashMap::new).get_mut(&path_key) {
                        // we have a local and a remote file
                        // set the joined remote file and the service
                        merged_file.remote = Some(remote_file.clone());
                        merged_file.remote_service = Some(remote_service.to_string());
                    } else {
                        // no local file, but we have a remote
                        result.entry(tracked_dir.clone()).or_insert_with(HashMap::new).insert(path_key.to_string(),
                        MergedFile {
                            local: None, 
                            remote: Some(remote_file.clone()),
                            remote_service: Some(remote_service.to_string())
                        });
                    }
            }
        }
        Ok(result)
    }


    // Get the status of the DataCollection, optionally with remotes.
    // 
    // Returns Result of BTreeMap of directory -> [ StatusEntry, ...]
    pub async fn status(&mut self, path_context: &Path, include_remotes: bool) -> Result<BTreeMap<String, Vec<StatusEntry>>> {
        let merged_files = self.merge(include_remotes).await?;

        let mut statuses_futures = FuturesUnordered::new();

        for (directory, inner_map) in merged_files.into_iter() {
            // this clone is to prevent a borrow issue due to async move below
            let files: Vec<_> = inner_map.values().cloned().collect();
            for mf in files {
                let directory_clone = directory.clone();
                statuses_futures.push(async move {
                    let status_entry = mf.status_entry(path_context, include_remotes).await.map_err(anyhow::Error::from)?;
                    Ok::<(String, StatusEntry), anyhow::Error>((directory_clone, status_entry))
                });
            }
        }

        let mut statuses = BTreeMap::new();

        let pb = ProgressBar::new(statuses_futures.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
                     .progress_chars("=> ")
                     .template("{spinner:.green} [{bar:40.green/white}] {pos:>}/{len} ({percent}%) eta {eta_precise:.green} {msg}")?
                    );


        let pb_clone = pb.clone();
        thread::spawn(move || {
            loop {
                pb_clone.tick();
                thread::sleep(Duration::from_millis(20));
            }
        });
        // process the futures as they become ready
        pb.set_message("Calculating MD5s...");
        while let Some(result) = statuses_futures.next().await {
            if let Ok((key, value)) = result {
                pb.set_message(format!("Calculating MD5s... {} done.", &value.name));
                statuses.entry(key).or_insert_with(Vec::new).push(value);
                pb.inc(1);
            } else {
                result?;
            }
        }

        pb.finish_with_message("Complete.");
        Ok(statuses)
    }

    pub async fn push(&mut self, path_context: &Path, overwrite: bool) -> Result<()> {
        // TODO before any push, we need to make sure that the project
        // status is "clean" e.g. nothing out of data.

        // Fetch all files as MergedFiles
        // note: this authenticates
        let all_files = self.merge(true).await?;

        let mut num_uploaded = 0;
        let mut current_skipped = Vec::new();
        let mut messy_skipped = Vec::new();
        let mut overwrite_skipped = Vec::new();
        let mut untracked_skipped = Vec::new();

        for (tracked_dir, files) in all_files.iter() {
            if let Some(remote) = self.remotes.get(tracked_dir) {
                for merged_file in files.values() {
                    let name = merged_file.name()?;
                    let path = PathBuf::from(tracked_dir).join(name).to_str().unwrap().to_string();
                    let local = merged_file.local.clone();

                    // if the file is not tracked or is remote-only, 
                    // we do not do anything
                    if local.as_ref().map_or(false, |mf| !mf.tracked) {
                        untracked_skipped.push(path);
                        continue;
                    }

                    // now we need to figure out whether to push the file, 
                    // which depends on the RemoteStatusCode and whether
                    // we should overwrite (TODO)
                    let do_upload = match merged_file.status(path_context)? {
                        RemoteStatusCode::NoLocal => {
                            return Err(anyhow!("Internal error: execution should not have reached this point, please report."));
                        },
                        RemoteStatusCode::Current => {
                            current_skipped.push(path);
                            false
                        },
                        RemoteStatusCode::Exists => {
                            // it exists on the remote, but we cannot
                            // compare MD5s. Push only if overwrite is true.
                            if !overwrite {
                                overwrite_skipped.push(path);
                            }
                            overwrite
                        },
                        RemoteStatusCode::MessyLocal => {
                            messy_skipped.push(path);
                            false
                        },
                        RemoteStatusCode::Invalid => {
                            return Err(anyhow!("A file ({:}) with RemoteStatusCode::Invalid was encountered. Please report.", path));
                        }, 
                        RemoteStatusCode::Different => {
                            // TODO if remote supports modification times,
                            // could do extra comparison here
                            info!("skipping {:} {:}", path, overwrite);
                            if !overwrite {
                                overwrite_skipped.push(path);
                            }
                            overwrite
                        },
                        RemoteStatusCode::DeletedLocal => {
                            // there is nothing to upload
                            print_warn!("A file ({:}) was skipped because it was deleted.", path);
                            false 
                        },
                        RemoteStatusCode::NotExists => true
                    };

                    if do_upload {
                        let data_file = local.ok_or(anyhow!("Internal error (do_upload() with MergedFile.local = None): please report."))?;
                        print_info!("uploading file {:?} to {}", data_file.path, remote.name());
                        remote.upload(&data_file, path_context, overwrite).await?;
                        num_uploaded += 1;
                    }

                }
            }
        }
        println!("Uploaded {}.", pluralize(num_uploaded as u64, "file"));
        let num_skipped = overwrite_skipped.len() + current_skipped.len() +
            messy_skipped.len() + untracked_skipped.len();
        println!("Skipped {} files:", num_skipped);
        if !untracked_skipped.is_empty() {
            println!("  Untracked: {}", pluralize(untracked_skipped.len() as u64, "file"));
            for path in untracked_skipped {
                println!("   - {:}", path);
            }
        }
        if !current_skipped.is_empty() {
            println!("  Remote file is indentical to local file: {}",
                     pluralize(current_skipped.len() as u64, "file"));
            for path in current_skipped {
                println!("   - {:}", path);
            }
        }
        if !overwrite_skipped.is_empty() {
            println!("  Would overwrite (use --overwrite to push): {}", 
                     pluralize(overwrite_skipped.len() as u64, "file"));
            for path in overwrite_skipped {
                println!("   - {:}", path);
            }
        }
        if !messy_skipped.is_empty() {
            println!("  Local is \"messy\" (manifest and file disagree): {}",
            pluralize(messy_skipped.len() as u64, "file"));
            for path in messy_skipped {
                println!("   - {:}", path);
            }
        }

        Ok(())
    }

    // Download all files
    //
    // TODO: code redundancy with the push method's tracking of
    // why stuff is skipped; split out info enum, etc.
    pub async fn pull(&mut self, path_context: &Path, overwrite: bool) -> Result<()> {
        let all_files = self.merge(true).await?;

        let mut downloads = Vec::new();

        let mut current_skipped = Vec::new();
        let mut messy_skipped = Vec::new();
        let mut overwrite_skipped = Vec::new();

        for (dir, merged_files) in all_files.iter() {
            for merged_file in merged_files.values().filter(|f| f.can_download()) {

                let path = merged_file.name()?;

                let do_download = match merged_file.status(path_context)? {
                    RemoteStatusCode::NoLocal => {
                        return Err(anyhow!("Internal error: execution should not have reached this point, please report."));
                    },
                    RemoteStatusCode::Current => {
                        current_skipped.push(path);
                        false
                    },
                    RemoteStatusCode::Exists => {
                        // it exists on the remote, but we cannot
                        // compare MD5s. Push only if overwrite is true.
                        if !overwrite {
                            overwrite_skipped.push(path);
                        }
                        overwrite
                    },
                    RemoteStatusCode::MessyLocal => {
                        messy_skipped.push(path);
                        false
                    },
                    RemoteStatusCode::Invalid => {
                        return Err(anyhow!("A file ({:}) with RemoteStatusCode::Invalid was encountered. Please report.", path));
                    }, 
                    RemoteStatusCode::Different => {
                        // TODO if remote supports modification times,
                        // could do extra comparison here
                        info!("skipping {:} {:}", path, overwrite);
                        if !overwrite {
                            overwrite_skipped.push(path);
                        }
                        overwrite
                    },
                    RemoteStatusCode::DeletedLocal => {
                        true
                    },
                    RemoteStatusCode::NotExists => true
                };

                if do_download { 
                    if let Some(remote) = self.remotes.get(dir) {
                        let info = remote.get_download_info(merged_file, path_context, overwrite)?;
                        let download = info.trauma_download()?;
                        downloads.push(download);
                    }
                }
            }
        }

        let style = ProgressBarOpts::new(
            Some("{spinner:.green} [{bar:40.green/white}] {pos:>}/{len} ({percent}%) eta {eta_precise:.green} {msg}".to_string()),
            Some("=> ".to_string()),
            true, true);

        let style_clone = style.clone();
        let style_opts = StyleOptions::new(style, style_clone);

        let total_files = downloads.len();
        if !downloads.is_empty() { 
            let downloader = DownloaderBuilder::new()
                .style_options(style_opts)
                .build();
            downloader.download(&downloads).await;
            println!("Downloaded {}.", pluralize(total_files as u64, "file"));
        } else {
            println!("No files downloaded.");
        }

        let num_skipped = overwrite_skipped.len() + current_skipped.len() +
            messy_skipped.len();
        println!("Skipped {} files. Reasons:", num_skipped);
        if !current_skipped.is_empty() {
            println!("  Remote file is indentical to local file: {}",
                     pluralize(current_skipped.len() as u64, "file"));
            for path in current_skipped {
                println!("   - {:}", path);
            }
        }
        if !overwrite_skipped.is_empty() {
            println!("  Would overwrite (use --overwrite to push): {}", 
                     pluralize(overwrite_skipped.len() as u64, "file"));
            for path in overwrite_skipped {
                println!("   - {:}", path);
            }
        }
        if !messy_skipped.is_empty() {
            println!("  Local is \"messy\" (manifest and file disagree): {}",
            pluralize(messy_skipped.len() as u64, "file"));
            for path in messy_skipped {
                println!("   - {:}", path);
            }
        }

        Ok(())
    }

}


#[cfg(test)]
mod tests {
    use crate::lib::api::figshare::{FIGSHARE_BASE_URL,FigShareAPI};
    use crate::lib::remote::Remote;
    use crate::lib::test_utilities::check_error;

    use super::{DataFile, DataCollection};
    use std::path::Path;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn mock_data_file() -> NamedTempFile {
        let temp_file = NamedTempFile::new().unwrap();
        temp_file
    }

    #[tokio::test]
    async fn test_datafile_new_with_nonexistent_path() {
        let nonexistent_path = "some/nonexistent/path".to_string();
        let path_context = Path::new("");

        let result = DataFile::new(nonexistent_path, &path_context);
        match result {
            Ok(_) => assert!(false, "Expected an error, but got Ok"),
            Err(err) => {
                assert!(err.to_string().contains("does not exist"),
                "Unexpected error: {:?}", err);
            }
        };
    }

    #[tokio::test]
    async fn test_md5() {
        let path_context = Path::new("");
        let mut file = mock_data_file();

        // Write some "data"
        writeln!(file, "Mock data.").unwrap();

        // Make a DataFile
        let path = file.path().to_string_lossy().to_string();
        let data_file = DataFile::new(path, &path_context).unwrap();

        // Compare MD5s
        let expected_md5 = "d3feb335769173b2db573413b0f6abf4".to_string();
        let observed_md5 = data_file.get_md5(&path_context).unwrap().unwrap();
        assert!(observed_md5 == expected_md5, "MD5 mismatch!");
    }


    #[tokio::test]
    async fn test_size() {
        let path_context = Path::new("");
        let mut file = mock_data_file();

        // Write some "data"
        writeln!(file, "Mock data.").unwrap();

        // Make a DataFile
        let path = file.path().to_string_lossy().to_string();
        let data_file = DataFile::new(path, &path_context).unwrap();

        // Let's also check size
        assert!(data_file.size == 11, "Size mismatch {:?} != {:?}!",
                data_file.size, 11);
    }



    #[tokio::test]
    async fn test_update_md5() {
        let path_context = Path::new("");
        let mut file = mock_data_file();

        // Write some "data"
        writeln!(file, "Mock data.").unwrap();

        // Make a DataFile
        let path = file.path().to_string_lossy().to_string();
        let mut data_file = DataFile::new(path, &path_context).unwrap();

        // Now, we change the data.
        writeln!(file, "Modified mock data.").unwrap();

        // Make sure the file MD5 is right
        let expected_md5 = "c6526ab1de615b49e53398ae5588bd00".to_string();
        let observed_md5 = data_file.get_md5(&path_context).unwrap().unwrap();
        assert!(observed_md5 == expected_md5);

        // Make sure the old MD5 is in the DataFile
        let old_md5 = "d3feb335769173b2db573413b0f6abf4".to_string();
        assert!(data_file.md5 == old_md5, "DataFile.md5 mismatch!");

        // Now update
        data_file.update_md5(path_context).unwrap();
        assert!(data_file.md5 == expected_md5, "DataFile.update_md5() failed!");
    }

    #[tokio::test]
    async fn test_update_size() {
        let path_context = Path::new("");
        let mut file = mock_data_file();

        // Write some "data"
        writeln!(file, "Mock data.").unwrap();

        // Make a DataFile
        let path = file.path().to_string_lossy().to_string();
        let mut data_file = DataFile::new(path, &path_context).unwrap();

        // Now, we change the data.
        writeln!(file, "Modified mock data.").unwrap();

        assert!(data_file.size == 11, "Initial size wrong!");

        data_file.update_size(path_context).unwrap();
        assert!(data_file.size == 31, "DataFile.update_size() wrong!");
    }


    #[test]
    fn test_register_remote_figshare() {
        let mut dc = DataCollection::new();

        let dir = "data/supplement".to_string();
        let result = FigShareAPI::new("Test remote", None);
        assert!(result.is_ok(), "FigShareAPI::new() resulted in error: {:?}", result);
        let figshare = result.unwrap();
        assert!(figshare.get_base_url() == FIGSHARE_BASE_URL, "FigShareAPI.base_url is not correct!");
        dc.register_remote(&dir, Remote::FigShareAPI(figshare)).unwrap();

        // check that it's been inserted
        assert!(dc.remotes.contains_key(&dir), "Remote not registered!");

        // Let's check that validate_remote_directory() is working
        let figshare = FigShareAPI::new("Another test remote", None).unwrap();
        let result = dc.register_remote(&dir, Remote::FigShareAPI(figshare));
        check_error(result, "already tracked");
    }

}
