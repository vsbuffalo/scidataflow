use std::path::{PathBuf,Path};
use anyhow::{anyhow,Result};
use std::fs::{metadata};
use serde_derive::{Serialize,Deserialize};
use serde::ser::SerializeMap;
use serde;
#[allow(unused_imports)]
use log::{info, trace, debug};
use chrono::prelude::*;
use std::collections::{HashMap,HashSet,BTreeMap};
use futures::future::join_all;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::fs;
use colored::*;

use crate::traits::Status;
use crate::utils::{format_mod_time,compute_md5, md5_status};
use crate::remote::{authenticate_remote,Remote,RemoteFile,RemoteStatusCode};

// The status of a local data file, *conditioned* on it being in the manifest.
#[derive(PartialEq,Clone)]
pub enum LocalStatusCode {
    Current,     // The MD5s between the file and manifest agree
    Modified,    // The MD5s disagree
    Deleted,     // The file is in the manifest but not file system
    Invalid      // Invalid state
}

#[derive(Clone)]
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
    fn include_remotes(&self) -> bool {
        self.remote_status.is_some()
    }
    pub fn color(&self, line: String) -> String {
        let tracked = self.tracked;
        let local_status = &self.local_status;
        let remote_status = &self.remote_status;
        let line = match (tracked, local_status, remote_status) {
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Current)) => line.green().to_string(),
            (Some(true), Some(LocalStatusCode::Current), None) => line.green().to_string(),
            // not tracked, but on remote
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Current)) => line.cyan().to_string(),
            // not tracked, not on remote
            (Some(false), Some(LocalStatusCode::Current), None) => line.yellow().to_string(),
            (Some(false), Some(LocalStatusCode::Current), Some(RemoteStatusCode::NotExists)) => line.yellow().to_string(),
            (None, Some(LocalStatusCode::Current), None) => line.green().to_string(),

            (Some(true), Some(LocalStatusCode::Modified), _)  => line.red().to_string(),
            (Some(false), Some(LocalStatusCode::Modified), _)  => line.red().to_string(),
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::NotExists))  => line.yellow().to_string(),
            (Some(true), Some(LocalStatusCode::Current), Some(RemoteStatusCode::Different))  => line.yellow().to_string(),
            (Some(false), Some(LocalStatusCode::Current), _)  => line.green().to_string(),
            _ => line.cyan().to_string()
        };
        line
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

        let mut columns = vec![
            self.name.clone(),
            local_status_msg.to_string(),
            md5_string,
            mod_time_pretty,
        ];

        if self.include_remotes() {
            let remote_status_msg = match &self.remote_status {
                Some(RemoteStatusCode::Current) => "current",
                Some(RemoteStatusCode::MessyLocal) => "messy local",
                Some(RemoteStatusCode::Different) => "different",
                Some(RemoteStatusCode::NotExists) => "not on remote",
                Some(RemoteStatusCode::NoLocal) => "unknown (messy remote)",
                Some(RemoteStatusCode::Exists) => "  ???  ",
                _ => "invalid"
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergedFile {
    pub local: Option<DataFile>,
    pub remote: Option<RemoteFile>
}


impl MergedFile {
    pub fn merge(data_file: &DataFile, remote_file: &RemoteFile) -> Result<MergedFile> {
        Ok(MergedFile {
            local: Some(data_file.clone()),
            remote: Some(remote_file.clone())
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

    pub fn has_remote(&self) -> Result<bool> {
        Ok(!self.remote.is_none())
    }

    pub fn is_tracked(&self) -> Option<bool> {
        self.local.as_ref().map(|data_file| data_file.tracked)
    }

    pub fn local_md5(&self, path_context: &PathBuf) -> Option<String> {
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

    pub fn local_remote_md5_mismatch(&self, path_context: &PathBuf) -> Option<bool> {
        let local_md5 = self.local_md5(path_context);
        let remote_md5 = self.remote_md5();
        match (remote_md5, local_md5) {
            (Some(remote), Some(local)) => Some(remote != local),
            _ => None,
        }
    }

    pub fn local_mod_time(&self, path_context: &PathBuf) -> Option<DateTime<Utc>> {
        self.local.as_ref()
            .and_then(|data_file| data_file
                      .get_mod_time(path_context).ok())
    }

    pub fn status(&self, path_context: &PathBuf) -> Result<RemoteStatusCode> {
        //let tracked = self.local.as_ref().map_or(None,|df| Some(df.tracked));

        // local status, None if no local file found
        let local_status = self.local
            .as_ref()
            .map_or(None, |local| local.status(path_context).ok());

        let md5_mismatch = self.local_remote_md5_mismatch(path_context);
    
        if !self.has_remote().unwrap_or(false) {
            return Ok(RemoteStatusCode::NotExists)
        }

        let status = match (&local_status, &md5_mismatch) {
            (Some(LocalStatusCode::Current), Some(true)) => {
                RemoteStatusCode::Current
            },
            (Some(LocalStatusCode::Current), Some(false)) => {
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
                RemoteStatusCode::MessyLocal
            },
            (Some(LocalStatusCode::Deleted), _) => {
                // Local file on file system does not exist,
                // but exists in the manifest. If the file is in 
                // the manifest and tracked a pull would pull it in.
                RemoteStatusCode::NoLocal
            },
            (_, _) => RemoteStatusCode::Invalid
        };

        Ok(status)
    }

    pub async fn status_entry(&self, path_context: &PathBuf) -> Result<StatusEntry> {
        let tracked = self.local.as_ref().map_or(None,|df| Some(df.tracked));
        let local_status = self.local
            .as_ref()
            .map_or(None, |local| local.status(path_context).ok());

        let remote_status = self.status(path_context)?;

        if self.local.is_none() && self.remote.is_none() {
            return Err(anyhow!("Internal error: MergedFile with no RemoteFile and DataFile set. Please report."));
        }

        Ok(StatusEntry {
            name: self.name()?,
            local_status,
            remote_status: Some(remote_status),
            tracked,
            remote_service: None,
            local_md5: self.local_md5(path_context),
            remote_md5: self.remote_md5(),
            manifest_md5: self.manifest_md5(),
            local_mod_time: self.local_mod_time(path_context)
        })
    }
}


impl DataFile {
    pub fn new(path: String, path_context: PathBuf) -> Result<DataFile> {
        let full_path = path_context.join(&path);
        let md5 = match compute_md5(&full_path)? {
            Some(md5) => md5,
            None => return Err(anyhow!("Could not compute MD5 as file does not exist")),
        };
        let size = metadata(full_path)
            .map_err(|err| anyhow!("Failed to get metadata for file {:?}: {}", path, err))?
            .len();
        Ok(DataFile {
            path: path,
            tracked: false, 
            md5: md5,
            size: size,
        })
    }

    pub fn full_path(&self, path_context: &PathBuf) -> Result<PathBuf> {
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
           .unwrap_or_else(|| path)
           .to_str()
           .unwrap_or("")
           .to_string())
    }

    pub fn get_md5(&self, path_context: &PathBuf) -> Result<Option<String>> {
        compute_md5(&self.full_path(path_context)?)
    }

    pub fn get_mod_time(&self, path_context: &PathBuf) -> Result<DateTime<Utc>> {
        let metadata = fs::metadata(self.full_path(path_context)?)?;
        let mod_time = metadata.modified()?.into();
        Ok(mod_time)
    }

    pub fn get_size(&self, path_context: &PathBuf) -> Result<u64> {
        // use metadata() method to get file metadata and extract size
        let size = metadata(&self.full_path(path_context)?)
            .map_err(|err| anyhow!("Failed to get metadata for file {:?}: {}", self.path, err))?
            .len();
        Ok(size)
    }

    pub fn is_alive(&self, path_context: &PathBuf) -> bool {
        path_context.join(&self.path).exists()
    }


    // Returns true if the file does not exist.
    pub fn is_changed(&self, path_context: &PathBuf) -> Result<bool> {
        match self.get_md5(path_context)? {
            Some(new_md5) => Ok(new_md5 != self.md5),
            None => Ok(true),
        }
    }

    pub fn status(&self, path_context: &PathBuf) -> Result<LocalStatusCode> {
        let is_alive = self.is_alive(path_context);
        let is_changed = self.is_changed(path_context)?;
        let local_status = match (is_changed, is_alive) {
            (false, true) => LocalStatusCode::Current,
            (true, true) => LocalStatusCode::Modified,
            (false, false) => LocalStatusCode::Deleted,  // Invalid? (TODO)
            (true, false) => LocalStatusCode::Deleted,
            _ => LocalStatusCode::Invalid,
        };
        Ok(local_status)
    }

    pub fn update_md5(&mut self, path_context: &PathBuf) -> Result<()> {
        let new_md5 = match self.get_md5(&path_context)? {
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

/// DataCollection structure for managing the data manifest 
/// and how it talks to the outside world.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataCollection {
    #[serde(serialize_with = "ordered_map")]
    pub files: HashMap<String, DataFile>,
    #[serde(serialize_with = "ordered_map")]
    pub remotes: HashMap<String, Remote>,
}

/// DataCollection methods: these should *only* be for 
/// interacting with the data manifest (including remotes).
impl DataCollection {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            remotes: HashMap::new(),
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
        self.files.insert(data_file.path.clone(), data_file);
        Ok(())
    }

    pub fn update(&mut self, filename: Option<&String>, path_context: PathBuf) -> Result<()> {
        match filename {
            Some(file) => {
                if let Some(data_file) = self.files.get_mut(file) {
                    data_file.update_md5(&path_context)?;
                    debug!("rehashed file {:?}", data_file.path);
                }
            }
            None => {
                // 
                let all_files: Vec<_> = self.files.keys().cloned().collect();
                for file in all_files {
                    if let Some(data_file) = self.files.get_mut(&file) {
                        data_file.update_md5(&path_context)?;
                        debug!("rehashed file {:?}", data_file.path);
                    }

                }

            }
        }
        Ok(())
    }

    pub fn register_remote(&mut self, dir: &String, remote: Remote) -> Result<()> {
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

    // Fetch all remote files
    pub async fn fetch(&mut self) -> Result<HashMap<String, HashMap<String,RemoteFile>>> {
        self.authenticate_remotes()?;
        let mut all_remote_files = HashMap::new();
        for (path, remote) in &self.remotes {
            let remote_files = remote.get_files_hashmap().await?;
            all_remote_files.insert(path.clone(), remote_files);
        }
        Ok(all_remote_files)
    }

    // Use a fetch to get all remote files (as RemoteFile), and merge these 
    // in with the local data files (DataFile) into a MergedFile struct.
    // Missing remote/local files are None.
    pub async fn merge(&mut self, include_remotes: bool) -> Result<HashMap<String, HashMap<String, MergedFile>>> {
        // directory -> (filename -> MergedFile)
        let mut result: HashMap<String, HashMap<String, MergedFile>> = HashMap::new();

        // Initialize the result with local files
        for (name, local_file) in &self.files {
            let dir = local_file.directory()?;
            result.entry(dir).or_insert_with(HashMap::new)
                .insert(name.clone(),
                MergedFile { local: Some(local_file.clone()), remote: None });
        }

        if !include_remotes {
            return Ok(result)
        }

        // iterate through each remote and retrieve remote files
        let all_remote_files = self.fetch().await?;
        for (tracked_dir, remote_files) in all_remote_files.iter() {
            // merge remote files with local files
            for (name, remote_file) in remote_files {
                // try to get the tracked directory; it doesn't exist make it
                if let Some(merged_file) = result.entry(tracked_dir.clone())
                    .or_insert_with(HashMap::new).get_mut(name) {
                        // 
                        merged_file.remote = Some(remote_file.clone());
                    } else {
                        result.entry(tracked_dir.clone()).or_insert_with(HashMap::new).insert(name.to_string(), MergedFile { local: None, remote: Some(remote_file.clone()) });
                    }
            }
        }
        Ok(result)
    }

    pub async fn status(&mut self, path_context: &PathBuf, include_remotes: bool) -> Result<BTreeMap<String, Vec<StatusEntry>>> {
        // get all merged files, used to compute the status
        let merged_files = self.merge(include_remotes).await?;

        let mut statuses = BTreeMap::new();

        // Get the StatusEntry async via join_all() for each 
        // MergedFile. The inner hash map has keys that are the 
        // file names (since this was use for join); these are not 
        // needed so they're ditched, leaving a 
        // BTreeMap<Vec<StatusEntry>>
        let statuses_futures: FuturesUnordered<_> = merged_files
            .into_iter()
            .map(|(outer_key, inner_map)| {
                // create a future for each merged_file, and collect the results
                async move {
                    let status_entries: Result<Vec<_>, _> = join_all(
                        inner_map.values()
                        // get the StatusEntry for each MergedFile
                        .map(|mf| async { mf.status_entry(path_context).await })
                        .collect::<Vec<_>>()
                        ).await.into_iter().collect();
                    status_entries.map(|entries| (outer_key, entries))
                }
            })
        .collect();

        let statuses_results: Vec<_> = statuses_futures.collect().await;

        for result in statuses_results {
            if let Ok((key, value)) = result {
                statuses.insert(key, value);
            } else {
                // Handle the error as needed
            }
        }

        Ok(statuses)
    }

}
