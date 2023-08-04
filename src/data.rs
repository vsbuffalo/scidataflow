use std::path::{PathBuf};
use serde_derive::{Serialize,Deserialize};
use log::{info, trace, debug};
use chrono::prelude::*;
use std::collections::HashMap;
use std::fs;

use crate::traits::Status;

use super::utils::compute_md5;
use super::remote::{Remote,FigShareAPI};

pub enum StatusCode {
   Current,
   Changed,
   DiskChanged,
   Updated,
   Deleted,
   Invalid
}

pub struct StatusEntry {
    pub code: StatusCode,
    pub cols: Vec<String>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataFile {
    path: PathBuf,
    tracked: bool,
    md5: Option<String>,
    modified: Option<DateTime<Utc>>,
}


impl DataFile {
    pub fn new(path: PathBuf, path_context: PathBuf) -> DataFile {
        let md5 = DataFile::get_md5(&path, &path_context);
        let modified = DataFile::get_mod_time(&path, &path_context);
        DataFile {
            path: path,
            tracked: true, 
            md5: Some(md5),
            modified: Some(modified)
        }
    }

    pub fn get_md5(path: &PathBuf, path_context: &PathBuf) -> String {
        let full_path = path_context.join(path.clone());
        compute_md5(&full_path).expect("cannot compute md5")
    }

    pub fn get_mod_time(path: &PathBuf, path_context: &PathBuf) -> DateTime<Utc> {
        let full_path = path_context.join(path.clone());
        let metadata = fs::metadata(&full_path)
            .expect("cannot compute modification time");
        metadata.modified()
            .expect("failed to get modification time")
            .into()
    }

    pub fn is_alive(&self, path_context: &PathBuf) -> bool {
        path_context.join(&self.path).exists()
    }

    pub fn is_changed(&self, path_context: &PathBuf) -> bool {
        let md5 = self.md5.as_deref() // TODO why deref needed?
            .unwrap_or_else(|| panic!("MD5 is not set!"));
        debug!("{:?} has changed!", self.path);
        md5 != DataFile::get_md5(&self.path, &path_context)
    }

    pub fn is_updated(&self, path_context: &PathBuf) -> bool {
        let modified = self.modified
            .unwrap_or_else(|| panic!("modification time is not set!"));
        modified != DataFile::get_mod_time(&self.path, &path_context)
    }

    pub fn touch(&mut self, path_context: &PathBuf) {
        let modified = DataFile::get_mod_time(&self.path, &path_context);
        self.modified = Some(modified);
    }
}


fn shorten(hash: &String, abbrev: Option<i32>) -> String {
    let n = abbrev.unwrap_or(hash.len() as i32) as usize;
    hash.chars().take(n).collect()
}

impl Status for DataFile {
    fn status(&self, path_context: &PathBuf, n: Option<i32>) -> StatusEntry {
        let is_updated = self.is_updated(path_context);
        let new_md5 = Some(DataFile::get_md5(&self.path, &path_context));

        let is_alive = Some(self.is_alive(&path_context));
        debug!("{:?}: old md5: {:?}, new md5: {:?}", self.path, self.md5, new_md5);
        let is_changed = self.md5 != new_md5;

        let old_modified = self.modified;
        let new_modified = DataFile::get_mod_time(&self.path, &path_context);
        debug!("{:?}: old mod: {:?}, new mod: {:?}", self.path, old_modified, new_modified);

        let md5_string = match (&self.md5, &new_md5) {
            (Some(old_md5), Some(new_md5)) if old_md5 != new_md5 => {
                format!("{} > {}", shorten(old_md5, n), shorten(new_md5, n))
            },
            (Some(md5), _) => format!("{}", shorten(md5, n)),
            _ => "".to_string(),
        };

        let modified_string = match (&self.modified, &new_modified) {
            (Some(old_modified), new_modified) if old_modified != new_modified => {
                format!("{} > {}", old_modified, new_modified)
            },
            (Some(modified), _) => format!("{}", modified),
            _ => "".to_string(),
        };

        // calculate the status code
        let code: StatusCode = match (&is_changed, &is_updated, &is_alive.unwrap()) {
            (false, false, true) => StatusCode::Current,
            (false, true, true) => StatusCode::Updated,
            (true, true, true) => StatusCode::Changed,
            (true, false, true) => StatusCode::DiskChanged,
            (false, false, false) => StatusCode::Deleted,
            _ => StatusCode::Invalid,
        };


        // append a status message column
        let status_msg = match code {
                StatusCode::Current => "current",
                StatusCode::Changed => "changed",
                StatusCode::DiskChanged => "changed",
                StatusCode::Updated => "updated, not changed",
                StatusCode::Deleted => "deleted",
                StatusCode::Invalid => "INVALID",
                _ => "ERROR",
            };

        let columns = vec![
            self.path.to_string_lossy().to_string(),
            status_msg.to_string(),
            md5_string,
            modified_string,
        ];

        StatusEntry { code: code, cols: columns }
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

    pub fn touch(&mut self, filename: Option<&String>, path_context: PathBuf) {
        match filename {
            Some(path) => {
                let path: PathBuf = path.into();
                if let Some(data_file) = self.files.get_mut(&path) {
                    data_file.touch(&path_context);
                    debug!("touched file {:?}", data_file.path);
                }
            }
            None => {
                // touch all files
                let keys: Vec<_> = self.files.keys().cloned().collect();
                for key in keys {
                    let path: PathBuf = key.into();
                    if let Some(data_file) = self.files.get_mut(&path) {
                        data_file.touch(&path_context);
                        debug!("touched file {:?}", data_file.path);
                    }

                }

            }
        }
    }

    pub fn register_remote(&mut self, dir: &String, service: &String) -> Result<(), String> {
        let service = service.to_lowercase();
        let remote = match service.as_str() {
            "figshare" => Ok(Remote::FigShareAPI(FigShareAPI::new())),
            _ => Err(format!("Service '{}' is not supported!", service))
        }?;

        let path = PathBuf::from(dir);
        if self.remotes.contains_key(&path) {
            return Err(format!("Directory '{}' is already being tracked; delete first.", dir));
        } else {
            self.remotes.insert(path, remote);
        }
        Ok(())
    }
}


