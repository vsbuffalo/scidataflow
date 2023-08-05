use std::path::{PathBuf};
use anyhow::{anyhow,Result};
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
    path: PathBuf,
    tracked: bool,
    md5: String,
    //modified: Option<DateTime<Utc>>,
}


impl DataFile {
    pub fn new(path: PathBuf, path_context: PathBuf) -> Result<DataFile> {
        let full_path = path_context.join(&path);
        let md5 = match compute_md5(&full_path)? {
            Some(md5) => md5,
            None => return Err(anyhow!("Could not compute MD5 as file does not exist")),
        };
        Ok(DataFile {
            path: path,
            tracked: true, 
            md5: md5,
        })
    }

    pub fn get_md5(path: &PathBuf, path_context: &PathBuf) -> Result<Option<String>> {
        compute_md5(path)
    }

    pub fn get_mod_time(path: &PathBuf, path_context: &PathBuf) -> Result<DateTime<Utc>> {
        let full_path = path_context.join(path.clone());
        let metadata = fs::metadata(&full_path)?;
        let mod_time = metadata.modified()?.into();
        Ok(mod_time)
    }

    pub fn is_alive(&self, path_context: &PathBuf) -> bool {
        path_context.join(&self.path).exists()
    }

    pub fn is_changed(&self, path_context: &PathBuf) -> Result<bool> {
        match DataFile::get_md5(&self.path, path_context)? {
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
        let new_md5 = match DataFile::get_md5(&self.path, &path_context)? {
            Some(md5) => md5,
            None => return Err(anyhow!("Cannot update MD5: file does not exist")),
        };
        self.md5 = new_md5;
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
        let new_md5 = DataFile::get_md5(&self.path, &path_context)?;
        let old_md5 = &self.md5;
        let mod_time = DataFile::get_mod_time(&self.path, &path_context)?;
        let status = self.status(path_context)?;

        let md5_string = match status {
            StatusCode::Current => format!("{}", shorten(&old_md5, n)),
            StatusCode::Changed => {
                match new_md5 {
                    Some(new_md5) => format!("{} > {}", shorten(&old_md5, n), shorten(&new_md5, n)),
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
            self.path.to_string_lossy().to_string(),
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
    pub files: HashMap<PathBuf, DataFile>,
    pub remotes: HashMap<PathBuf, Remote>,
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

    pub fn register(&mut self, data_file: DataFile) {
        self.files.insert(data_file.path.clone(), data_file);
    }

    pub fn update(&mut self, filename: Option<&String>, path_context: PathBuf) {
        match filename {
            Some(path) => {
                let path: PathBuf = path.into();
                if let Some(data_file) = self.files.get_mut(&path) {
                    data_file.update_md5(&path_context);
                    debug!("rehashed file {:?}", data_file.path);
                }
            }
            None => {
                // 
                let keys: Vec<_> = self.files.keys().cloned().collect();
                for key in keys {
                    let path: PathBuf = key.into();
                    if let Some(data_file) = self.files.get_mut(&path) {
                        data_file.update_md5(&path_context);
                        debug!("rehashed file {:?}", data_file.path);
                    }

                }

            }
        }
    }

    pub fn register_remote(&mut self, dir: &String, remote: Remote) -> Result<()> {
        let path = PathBuf::from(dir);
        if self.remotes.contains_key(&path) {
            let msg = anyhow!("Directory '{}' is already tracked in the \
                              data manifest. You can manually delete it \
                              and re-add.", dir);
            return Err(msg);
        } else {
            self.remotes.insert(path, remote);
        }
        Ok(())
    }

    pub fn get_remote(&mut self, dir: &String) -> Result<&Remote, String> {
        let path = PathBuf::from(dir);
        match self.remotes.get(&path) {
            Some(remote) => Ok(remote),
            None => Err("No such remote".to_string()),
        }
    }

}


