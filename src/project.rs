use std::collections::HashMap;
use std::fs::{File,metadata,canonicalize};
use anyhow::{anyhow,Result};
use std::{env, default};
use std::path::{Path,PathBuf};
use log::{info, trace, debug};
use reqwest::Response;
use std::io::{Write};

use super::remote::{Remote,FigShareAPI};
use super::data::{DataFile,DataCollection};
use super::utils::{load_file,print_status};
use super::remote::{AuthKeys,authenticate_remote,ResponseResult,ResponseResults};
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

    pub fn new() -> Result<Self> {
        let manifest = Project::get_manifest();
        let data = Project::load(&manifest)?;
        let proj = Project { manifest, data };
        Ok(proj)
    }

    pub fn name(&self) -> String {
        self.manifest
            .parent()
            .and_then(|path| path.file_name())
            .map(|os_str| os_str.to_string_lossy().into_owned())
            .unwrap_or_else(|| panic!("invalid project location: is it in root?"))
    }

    pub fn init() -> Result<()> {
        // the new manifest should be in the present directory
        let manifest: PathBuf = PathBuf::from(MANIFEST);
        let found_manifest = find_config(None, MANIFEST);
        if manifest.exists() || found_manifest.is_none() {
            return Err(anyhow!("Project already initialized. Manifest file already exists."));
        } else {
            let data = DataCollection::new();
            let proj = Project { manifest, data };
            // save to create the manifest
            proj.save()?;
        }
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
        let contents = load_file(&manifest);

        if contents.trim().is_empty() {
            // empty manifest, just create a new one
            return Ok(DataCollection::new());
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

    pub fn relative_path(&self, path: &Path) -> PathBuf {
        let cwd = env::current_dir().expect("Failed to get current directory");
        let rel_dir = cwd.strip_prefix(self.path_context()).unwrap().to_path_buf();
        let relative_path = rel_dir.join(path);
        debug!("relative_path = {:?}", relative_path);
        relative_path
    }

    pub fn relative_path_string(&self, path: &Path) -> Result<String> {
        Ok(self.relative_path(path).to_string_lossy().to_string())
    }

    pub fn status(&self) -> Result<()> {
        let abbrev = Some(8);
        let mut rows: Vec<StatusEntry> = Vec::new();
        for value in self.data.files.values() {
            let entry = value.status_info(&self.path_context(), abbrev)?;
            rows.push(entry);
        }
        print_status(rows, Some(&self.data.remotes));
        Ok(())
    }

    pub fn stats(&self) -> Result<()> {
        let mut rows: Vec<StatusEntry> = Vec::new();
        for (key, data_file) in self.data.files.iter() {
            let file_path = self.resolve_path(&key);

            let size = format_bytes(data_file.get_size(&self.path_context())?);

            let cols = vec![key.clone(), size];
            let entry = StatusEntry { status: StatusCode::Invalid, cols: cols };
            rows.push(entry);
        }
        print_status(rows, None);
        Ok(())
    }


    pub fn add(&mut self, filepath: &String) -> Result<()> {
        let filename = self.relative_path_string(Path::new(filepath))?;

        let data_file = DataFile::new(filename, self.path_context())?;
        self.data.register(data_file);
        self.save()
    }

    pub fn update(&mut self, filepath: Option<&String>) -> Result<()> {
        let path_context = self.path_context();
        self.data.update(filepath, path_context);
        self.save()
    }

    pub async fn link(&mut self, dir: &String, service: &String, 
                      key: &String, name: &Option<String>) -> Result<()> {
        // (0) get the relative directory path
        let dir = self.relative_path_string(Path::new(dir));
        // (1) save the auth key to home dir
        let mut auth_keys = AuthKeys::new();
        auth_keys.add(service, key);

        // (2) create a new remote 
        let service = service.to_lowercase();
        let mut remote = match service.as_str() {
            "figshare" => Ok(Remote::FigShareAPI(FigShareAPI::new())),
            _ => Err(anyhow!("Service '{}' is not supported!", service))
        }?;
    
        // (3) authenticate remote
        authenticate_remote(&mut remote)?;
        // (4) associate a project (either by creating it, or finding it on
        // FigShare)
        let default_name = self.name();
        let project_id = remote.set_project(name.as_ref().unwrap_or(&default_name)).await?;

        // (4) register the remote in the manifest
        self.data.register_remote(&dir?, remote)?;
        self.save()
    }

    pub async fn ls(&mut self) -> Result<()> {
        for (key, remote) in &mut self.data.remotes {
            //let all_projects: ResponseResults = remote.get_projects().await;
            //match all_projects {
            //    Ok(projects) => {
            //        for project in projects {
            //            println!("project ID = {:?}", project.get("id"))
            //        }
            //    },
            //    Err(err) => eprintln!("Error while getting projects: {}", err),
            //}
            authenticate_remote(remote)?;
            let files = remote.get_files().await?;
            println!("{} files:\n{:?}", key, files);
        }
        Ok(())
    }

    pub fn untrack(&mut self, filepath: &String) -> Result<()> {
        self.data.untrack_file(filepath)?;
        self.save()
    }

    pub fn track(&mut self, filepath: &String) -> Result<()> {
        self.data.track_file(filepath)?;
        self.save()
    }

    pub async fn push(&mut self) -> Result<()> {
        // TODO before any push, we need to make sure that the project
        // status is "clean" e.g. nothing out of data.
        for (key, remote) in &mut self.data.remotes {
            authenticate_remote(remote)?;
        }
        
        for (key, remote) in &self.data.remotes {
            for (path, data_file) in &self.data.files {
                info!("uploading file {:?} to {:}", path, remote.name());
                remote.upload(&data_file).await?;
            }
        }
        Ok(())
    }

}

