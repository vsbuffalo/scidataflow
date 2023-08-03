use std::fs::{File,metadata,canonicalize};
use std::env;
use std::path::{Path,PathBuf};
use log::{info, trace, debug};
use std::io::{Result,Write};

use super::data::{DataFile,DataCollection};
use crate::data::StatusEntry;
use super::utils::{load_file,print_status};
use crate::traits::Status;

const MANIFEST: &str = "data_manifest.yml";


pub fn find_config(start_dir: Option<&PathBuf>, filename: &str) -> Option<PathBuf> {
    let mut current_dir = match start_dir {
        Some(dir) => dir.to_path_buf(),
        None => env::current_dir().expect("Failed to get current directory")
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

pub struct Project {
    manifest: PathBuf,
    data: DataCollection
}


impl Project {
    fn get_manifest() -> PathBuf {
        find_config(None, MANIFEST)
            .expect("SciFlow not initialized.")
    }

    pub fn new() -> Self {
        let manifest = Project::get_manifest();
        let data = Project::load(&manifest);
        let mut proj = Project { manifest, data };
        proj
    }

    pub fn init() {
        // the new manifest should be in the present directory
        let manifest: PathBuf = PathBuf::from(MANIFEST);
        let data = DataCollection::new();
        let mut proj = Project { manifest, data };
        // save to create the manifest
        proj.save();
    }

    pub fn save(&self) -> Result<()> {
        let serialized_data = serde_yaml::to_string(&self.data)
            .expect("Failed to serialize data manifest!");
        let mut file = File::create(self.manifest.clone())?;
        write!(file, "{}", serialized_data)?;
        Ok(())
    }

    fn load(manifest: &PathBuf) -> DataCollection {
        let contents = load_file(&manifest);

        if contents.trim().is_empty() {
            // empty manifest, just create a new one
            return DataCollection::new();
        }

        match serde_yaml::from_str(&contents) {
            Ok(data) => data,
            Err(_) => {
                panic!("{} is malformed!", MANIFEST);
            }
        }
    }

    /// Get the absolute path context of the current project.
    pub fn path_context(&self) -> PathBuf {
        let path = self.manifest.parent().unwrap().to_path_buf();
        debug!("path_context = {:?}", path);
        path
    }

    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        let full_path = self.path_context().join(path);
        let resolved_path = canonicalize(full_path).unwrap();
        debug!("resolved_path = {:?}", resolved_path);
        resolved_path
    }

    /// For a given file somewhere in the project, return the path
    /// relative to the root.
    pub fn relative_path(&self, path: &Path) -> PathBuf {
        let cwd = env::current_dir().expect("Failed to get current directory");
        let rel_dir = cwd.strip_prefix(self.path_context()).unwrap().to_path_buf();
        let relative_path = rel_dir.join(path);
        debug!("relative_path = {:?}", relative_path);
        relative_path
    }

    pub fn status(&self) {
        let abbrev = Some(8);
        let mut rows: Vec<StatusEntry> = Vec::new();
        for (key, value) in &self.data.files {
            let entry = value.status(&self.path_context(), abbrev);
            rows.push(entry);
        }
        print_status(rows);
    }


    pub fn add(&mut self, filepath: &String) {
        let msg = format!("cannot resolve path to {}", filepath);
        let filename = self.relative_path(Path::new(filepath));

        self.data.register(DataFile::new(filename.clone(), self.path_context()));
        self.save();
    }
}

