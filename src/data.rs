use std::path::{PathBuf};
use anyhow::{anyhow,Result};
use std::fs::{metadata};
use serde_derive::{Serialize,Deserialize};
use log::{info, trace, debug};
use chrono::prelude::*;
use std::collections::HashMap;
use std::fs;

use crate::traits::Status;

use super::utils::{format_mod_time,compute_md5};
use super::remote::{Remote,FigShareAPI};

pub enum StatusCode {
   Current,
   Changed,
   Deleted,
   Invalid
}

pub struct StatusEntry {
    pub status: StatusCode,
    pub cols: Vec<String>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataFile {
    pub path: String,
    pub tracked: bool,
    pub md5: String,
    pub size: u64,
    pub remote_id: Option<String>,
    //modified: Option<DateTime<Utc>>,
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
            remote_id: None,
        })
    }

    pub fn full_path(&self, path_context: &PathBuf) -> Result<PathBuf> {
        Ok(path_context.join(self.path.clone()))
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

    pub fn status(&self, path_context: &PathBuf) -> Result<StatusCode> {
        let is_alive = self.is_alive(path_context);
        let is_changed = self.is_changed(path_context)?;
        Ok(match (is_changed, is_alive) {
            (false, true) => StatusCode::Current,
            (true, true) => StatusCode::Changed,
            (false, false) => StatusCode::Deleted,
            _ => StatusCode::Invalid,
        })
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

fn shorten(hash: &String, abbrev: Option<i32>) -> String {
    let n = abbrev.unwrap_or(hash.len() as i32) as usize;
    hash.chars().take(n).collect()
}

impl Status for DataFile {
    fn status_info(&self, path_context: &PathBuf, n: Option<i32>) -> Result<StatusEntry> {
        //let is_updated = self.is_updated(path_context);
        let new_md5 = self.get_md5(path_context)?;
        let old_md5 = &self.md5;
        let mod_time = self.get_mod_time(path_context)?;
        let status = self.status(path_context)?;

        let md5_string = match status {
            StatusCode::Current => format!("{}", shorten(&old_md5, n)),
            StatusCode::Changed => {
                match new_md5 {
                    Some(new_md5) => format!("{}→{}", shorten(&old_md5, n), shorten(&new_md5, n)),
                    None => return Err(anyhow!("Error: new MD5 not available")),
                }
            },
            _ => "".to_string(),
        };

        let mod_time_pretty = format_mod_time(mod_time);

        // append a status message column
        let status_msg = match status {
            StatusCode::Current => "current",
            StatusCode::Changed => "changed",
            StatusCode::Deleted => "deleted",
            StatusCode::Invalid => "invalid",
        };

        let columns = vec![
            self.path.clone(),
            status_msg.to_string(),
            md5_string,
            mod_time_pretty,
        ];

        Ok(StatusEntry { status: status, cols: columns })
    }
}

/// DataCollection structure for managing the data manifest 
/// and how it talks to the outside world.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataCollection {
    pub files: HashMap<String, DataFile>,
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
        if self.remotes.contains_key(dir) {
            let msg = anyhow!("Directory '{}' is already tracked in the \
                              data manifest. You can manually delete it \
                              and re-add.", dir);
            return Err(msg);
        } else {
            self.remotes.insert(dir.to_string(), remote);
        }
        Ok(())
    }

    pub fn get_remote(&mut self, dir: &String) -> Result<&Remote> {
        match self.remotes.get(dir) {
            Some(remote) => Ok(remote),
            None => Err(anyhow!("No such remote")),
        }
    }
    pub fn track_file(&mut self, filepath: &String) -> Result<()> {
        let data_file = self.files.get_mut(filepath);
        match data_file {
            None => Err(anyhow!("Cannot get file '{}' from the data manifest.", filepath)),
            Some(data_file) => data_file.set_tracked()
        }
    }
    pub fn untrack_file(&mut self, filepath: &String) -> Result<()> {
        let data_file = self.files.get_mut(filepath);
        match data_file {
            None => Err(anyhow!("Cannot get file '{}' from the data manifest.", filepath)),
            Some(data_file) => data_file.set_untracked()
        }
    }

}


