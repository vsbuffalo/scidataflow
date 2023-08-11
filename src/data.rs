use std::path::{PathBuf,Path};
use anyhow::{anyhow,Result};
use std::fs::{metadata};
use serde_derive::{Serialize,Deserialize};
use serde::ser::SerializeMap;
use serde;
#[allow(unused_imports)]
use log::{info, trace, debug};
use chrono::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::traits::Status;

use super::utils::{format_mod_time,compute_md5};
use super::remote::{authenticate_remote,Remote,RemoteStatusCode,RemoteFile};

#[derive(PartialEq,Clone)]
pub enum LocalStatusCode {
   Current,
   Modified,
   Deleted,
   Invalid
}

#[derive(Clone)]
pub struct StatusEntry {
    pub local_status: LocalStatusCode,
    pub remote_status: Option<RemoteStatusCode>,
    pub tracked: Option<bool>, // None indicates that no remote 
    pub cols: Option<Vec<String>>,
    pub remote_service: Option<String>
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
        if data_file.basename()? != remote_file.name {
            return Err(anyhow!("Mismatch between local and remote file names"));
        }

        Ok(MergedFile {
            local: Some(data_file.clone()),
            remote: Some(remote_file.clone())
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

    pub fn is_changed(&self, path_context: &PathBuf) -> Result<bool> {
        match self.get_md5(path_context)? {
            Some(new_md5) => Ok(new_md5 != self.md5),
            None => Ok(true),
        }
    }

    pub fn status(&self, path_context: &PathBuf) -> Result<StatusEntry> {
        let is_alive = self.is_alive(path_context);
        let is_changed = self.is_changed(path_context)?;
        let local_status = match (is_changed, is_alive) {
            (false, true) => LocalStatusCode::Current,
            (true, true) => LocalStatusCode::Modified,
            (false, false) => LocalStatusCode::Deleted,
            _ => LocalStatusCode::Invalid,
        };
        Ok(StatusEntry { 
            local_status,
            remote_status: None,
            tracked: None,
            remote_service: None,
            cols: None
        }) 
    }
    // Do a merge on paths, and fold in remote status.
    // Merge is left join, where there will be some files on remote 
    // that are not DataFiles -- DataFiles must represent a fixed file 
    // that is *tracked* in the manifest. 
    pub async fn status_with_remotes(&self, path_context: &PathBuf, remotes: Option<&HashMap<String,Remote>>) -> Result<StatusEntry> {
        let mut status_entry = self.status(&path_context)?;

        let mut found_remote_files: Vec<(String, RemoteStatusCode)> = Vec::new();
        let mut remote_names: HashMap<String, Vec<String>> = HashMap::new();

        if let Some(remotes_map) = remotes {
            // iterate through all remotes, looking for one that contains this DataFile.
            for (path, remote) in remotes_map.iter() {
                let remote_status = remote.file_status(&self, &path_context).await?;
                let (file_name, _) = &remote_status;

                found_remote_files.push(remote_status.clone());

                // collect the names of the remotes where each file was found.
                remote_names.entry(file_name.clone()).or_default().push(remote.name().to_string());
            }

            // raise error if the file is tracked by multiple remotes
            for (file_name, remote_list) in remote_names.iter() {
                if remote_list.len() > 1 {
                    return Err(anyhow!("File '{}' is tracked in multiple remotes: {:?}", file_name, remote_list));
                }
            }

            // extract out the singular remote status code and assign
            if let Some((service, remote_status_code)) = found_remote_files.first() {
                status_entry.remote_status = Some(remote_status_code.clone());
                status_entry.remote_service = Some(service.clone());
                status_entry.tracked = Some(self.tracked);
            }
        } else {
            // No remotes given.
            status_entry.remote_status = None;
            status_entry.remote_service = None;
            status_entry.tracked = None;
        }

        Ok(status_entry)
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

    pub async fn status_info(&self, path_context: &PathBuf, remotes: Option<&HashMap<String,Remote>>, n: Option<i32>) -> Result<StatusEntry> {
        //let is_updated = self.is_updated(path_context);
        let new_md5 = self.get_md5(path_context)?;
        let old_md5 = &self.md5;
        let mod_time = self.get_mod_time(path_context)?;
        let status = self.status_with_remotes(path_context, remotes).await?;
        let local_status = status.local_status;
        let remote_status = if remotes.is_some() { status.remote_status } else { None };
        let remote_service = if remotes.is_some() { status.remote_service } else { None };

        let md5_string = match local_status {
            LocalStatusCode::Current => format!("{}", shorten(&old_md5, n)),
            LocalStatusCode::Modified => {
                match new_md5 {
                    Some(new_md5) => format!("{} → {}", shorten(&old_md5, n), shorten(&new_md5, n)),
                    None => return Err(anyhow!("Error: new MD5 not available")),
                }
            },
            _ => "".to_string(),
        };

        let mod_time_pretty = format_mod_time(mod_time);

        // append a local status message column
        let local_status_msg = match local_status {
            LocalStatusCode::Current => "current",
            LocalStatusCode::Modified => "changed",
            LocalStatusCode::Deleted => "deleted",
            LocalStatusCode::Invalid => "invalid",
        };

        let mut columns = vec![
            self.path.clone(),
            local_status_msg.to_string(),
            md5_string,
            mod_time_pretty,
        ];

        // if we have a remote status (e.g. we talked to remotes)
        // we add a column
        if let Some(status) = &remote_status {
            let remote_status_msg = match status {
                RemoteStatusCode::NotExists => "not on remote",
                RemoteStatusCode::Current => "current",
                RemoteStatusCode::MD5Mismatch => "remote has different version",
                RemoteStatusCode::NoMD5 => "no MD5 on remote",
                RemoteStatusCode::Invalid => "invalid",
            };
            let tracking_status = if self.tracked { "" } else { "not tracked" };
            let remote_status_msg = if self.tracked { remote_status_msg } else {""};
            columns.push(format!("{}{}", tracking_status, remote_status_msg));
        }

        Ok(StatusEntry {
            local_status: local_status.clone(),
            remote_status: remote_status.clone(),
            tracked: Some(self.tracked),
            remote_service,
            cols: Some(columns),
        })
    }
}

fn shorten(hash: &String, abbrev: Option<i32>) -> String {
    let n = abbrev.unwrap_or(hash.len() as i32) as usize;
    hash.chars().take(n).collect()
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


    pub async fn merge(&mut self) -> Result<HashMap<String, HashMap<String, MergedFile>>> {
        let mut result: HashMap<String, HashMap<String, MergedFile>> = HashMap::new();

        // Initialize the result with local files
        for (name, local_file) in &self.files {
            let dir = local_file.directory()?;
            result.entry(dir).or_insert_with(HashMap::new)
                .insert(name.clone(),
                MergedFile { local: Some(local_file.clone()), remote: None });
        }

        // iterate through each remote and retrieve remote files
        let all_remote_files = self.fetch().await?;
        for (tracked_dir, remote_files) in all_remote_files.iter() {
            // merge remote files with local files
            for (name, remote_file) in remote_files {
                // try to get the tracked directory; it doesn't exist make it
                if let Some(merged_file) = result.entry(tracked_dir.clone())
                    .or_insert_with(HashMap::new).get_mut(name) {
                    merged_file.remote = Some(remote_file.clone());
                } else {
                    result.entry(tracked_dir.clone()).or_insert_with(HashMap::new).insert(name.to_string(), MergedFile { local: None, remote: Some(remote_file.clone()) });
                }
            }
        }
        Ok(result)
    }

}


