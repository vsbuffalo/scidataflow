use std::alloc::System;
use std::fmt;
use std::path::{Path,PathBuf};
use serde_derive::{Serialize,Deserialize};
use serde_yaml;
use serde::ser::Serializer;
use serde::{Serialize, Deserialize, Deserializer};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use chrono::prelude::*;
use std::collections::HashMap;
use timeago::Formatter;
use std::fs;

use crate::traits::Status;

use super::utils::compute_md5;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Remote {
    service: String,
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

    pub fn is_changed(&self, path_context: PathBuf) -> bool {
        let md5 = self.md5.as_deref() // TODO why deref needed?
            .unwrap_or_else(|| panic!("MD5 is not set!"));
        md5 != DataFile::get_md5(&self.path, &path_context)
    }

    pub fn is_updated(&self, path_context: PathBuf) -> bool {
        let modified = self.modified
            .unwrap_or_else(|| panic!("modification time is not set!"));
        modified != DataFile::get_mod_time(&self.path, &path_context)
    }
}


fn shorten(hash: &String, abbrev: Option<i32>) -> String {
    let n = abbrev.unwrap_or(hash.len() as i32) as usize;
    hash.chars().take(n).collect()
}

impl Status for DataFile {
    fn status(&self, path_context: &PathBuf, n: Option<i32>) -> String {
        let is_updated = self.is_updated(path_context.to_path_buf());
        let mut new_md5: Option<String> = None;
        if is_updated {
            // lazy hashing -- if no modfication time change
            // assume not changed (TODO: option?)
            new_md5 = Some(DataFile::get_md5(&self.path, &path_context));
        }
        let is_changed = self.md5 != new_md5;

        let new_modified = DataFile::get_mod_time(&self.path, &path_context);

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

        let status_string = match (&self.md5, &new_md5, &self.modified, &new_modified) {
            (Some(old_md5), Some(new_md5), _, _) if old_md5 != new_md5 => "changed",
            (_, _, Some(old_modified), new_modified) if old_modified != new_modified => "modified, not changed",
            _ => "current",
        };

        let columns = vec![
            self.path.to_string_lossy().to_string(),
            md5_string,
            modified_string,
            status_string.to_string(),
        ];

        columns.join("\t")

    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataCollection {
    pub files: HashMap<PathBuf, DataFile>,
    pub remotes: HashMap<PathBuf, Remote>,

}

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

}


