use std::fs::{File,metadata,canonicalize};
use std::env;
use std::path::{Path,PathBuf};
use log::{info, trace, debug};
use std::io::{Write};

use super::data::{DataFile,DataCollection};
use super::utils::{load_file,print_status};
use super::remote::{AuthKeys,initialize_remotes};
use crate::data::{StatusEntry,StatusCode};
use crate::traits::Status;
use crate::utils::{format_bytes, print_fixed_width};

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

    pub fn new() -> Result<Self, String> {
        let manifest = Project::get_manifest();
        let data = Project::load(&manifest)?;
        let proj = Project { manifest, data };
        Ok(proj)
    }

    pub fn init() -> Result<(), String> {
        // the new manifest should be in the present directory
        let manifest: PathBuf = PathBuf::from(MANIFEST);
        let found_manifest = find_config(None, MANIFEST);
        if manifest.exists() || found_manifest.is_none() {
            return Err(String::from("Project already initialized. Manifest file already exists."));
        } else {
            let data = DataCollection::new();
            let proj = Project { manifest, data };
            // save to create the manifest
            proj.save().map_err(|e| format!("Failed to save the project: {}", e))?;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        // Serialize the data
        let serialized_data = serde_yaml::to_string(&self.data)
            .map_err(|err| format!("Failed to serialize data manifest: {}", err))?;

        // Create the file
        let mut file = File::create(&self.manifest)
            .map_err(|err| format!("Failed to open file '{:?}': {}", self.manifest, err))?;

        // Write the serialized data to the file
        write!(file, "{}", serialized_data)
            .map_err(|err| format!("Failed to write data manifest: {}", err))?;

        Ok(())
    }

    fn load(manifest: &PathBuf) -> Result<DataCollection, String> {
        let contents = load_file(&manifest);

        if contents.trim().is_empty() {
            // empty manifest, just create a new one
            return Ok(DataCollection::new());
        }

        let mut data = serde_yaml::from_str(&contents)
            .map_err(|e| format!("{} is malformed!\nError: {:?}", MANIFEST, e))?;
        initialize_remotes(&mut data)?;
        Ok(data)
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

    pub fn relative_path(&self, path: &Path) -> PathBuf {
        let cwd = env::current_dir().expect("Failed to get current directory");
        let rel_dir = cwd.strip_prefix(self.path_context()).unwrap().to_path_buf();
        let relative_path = rel_dir.join(path);
        debug!("relative_path = {:?}", relative_path);
        relative_path
    }

    pub fn status(&self) -> Result<(), String> {
        let abbrev = Some(8);
        let mut rows: Vec<StatusEntry> = Vec::new();
        for value in self.data.files.values() {
            let entry = value.status(&self.path_context(), abbrev);
            rows.push(entry);
        }
        print_status(rows, Some(&self.data.remotes));
        Ok(())
    }

    pub fn stats(&self) -> Result<(), String> {
        let mut rows: Vec<StatusEntry> = Vec::new();
        for key in self.data.files.keys() {
            let file_path = self.resolve_path(&key);

            // use metadata() method to get file metadata and extract size
            let metadata = metadata(&file_path)
                .map_err(|err| format!("Failed to get metadata for file {:?}: {}", file_path, err))?;

            let size = format_bytes(metadata.len());

            let cols = vec![key.to_string_lossy().to_string(), size];
            let entry = StatusEntry { code: StatusCode::Invalid, cols: cols };
            rows.push(entry);
        }
        print_status(rows, None);
        Ok(())
    }


    pub fn add(&mut self, filepath: &String) -> Result<(), String> {
        let filename = self.relative_path(Path::new(filepath));

        self.data.register(DataFile::new(filename.clone(), self.path_context()));
        self.save()?;
        Ok(())
    }

    pub fn touch(&mut self, filepath: Option<&String>) -> Result<(), String> {
        let path_context = self.path_context();
        self.data.touch(filepath, path_context);
        self.save()?;
        Ok(())
    }

    pub fn link(&mut self, dir: &String, service: &String, key: &String) -> Result<(), String> {
        // do two things:
        // (1) save the auth key to home dir
        // (2) register the remote 
        let mut auth_keys = AuthKeys::new();
        auth_keys.add(service, key);
        self.data.register_remote(dir, service)?;
        self.save()?;
        Ok(())
    }

    pub async fn ls(&mut self) -> Result<(), String> {
        for (key, remote) in &self.data.remotes {
            match remote.get_project().await {
                Ok(project_id) => println!("project ID = {:?}", project_id),
                Err(err) => eprintln!("Error while getting project ID: {}", err),
            }
        }
        Ok(())
    }

}

