use std::fs::{File,metadata,canonicalize};
use anyhow::{anyhow,Result};
use std::{env};
use std::path::{Path,PathBuf};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::io::{Write};
use colored::Colorize;

use crate::{print_warn,print_info};
use crate::data::{DataFile,DataCollection};
use crate::utils::{load_file,print_status, print_fixed_width_status};
use crate::remote::{AuthKeys,authenticate_remote,RemoteStatusCode};
use crate::remote::Remote;
use crate::figshare::FigShareAPI;
use crate::data::{StatusEntry,LocalStatusCode};
use crate::traits::Status;
use crate::utils::{format_bytes};

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

    pub fn relative_path(&self, path: &Path) -> Result<PathBuf> {
        let absolute_path = canonicalize(path)?;
        //ensure_directory(&absolute_path)?;
        let path_context = canonicalize(self.path_context())?;

        // Compute relative path directly using strip_prefix
        match absolute_path.strip_prefix(&path_context) {
            Ok(rel_path) => Ok(rel_path.to_path_buf()),
            Err(_) => Err(anyhow::anyhow!("Failed to compute relative path")),
        }
    }

    pub fn relative_path_string(&self, path: &Path) -> Result<String> {
        Ok(self.relative_path(path)?.to_string_lossy().to_string())
    }

    pub async fn status(&mut self, include_remotes: bool) -> Result<()> {
        // if include_remotes (e.g. --remotes) is set, we need to merge
        // in the remotes, so we authenticate first and then get them.
        let path_context = &canonicalize(self.path_context())?;
        let status_rows = self.data.status(path_context, include_remotes).await?;
        //let remotes: Option<_> = include_remotes.then(|| &self.data.remotes);
        print_status(status_rows, Some(&self.data.remotes));
        Ok(())
    }

    pub fn is_clean(&self) -> Result<bool> {
        for data_file in self.data.files.values() {
            let status = data_file.status(&self.path_context())?;
            if status != LocalStatusCode::Current {
                return Ok(false);
            }
        }
        Ok(true)
    }
/*
    pub fn stats(&self) -> Result<()> {
        let mut rows: Vec<StatusEntry> = Vec::new();
        for (key, data_file) in self.data.files.iter() {
            let size = format_bytes(data_file.get_size(&self.path_context())?);
            let cols = vec![key.clone(), size];
            // TODO use different more general struct?
            // Or print_fixed_width should be a trait?
            let entry = StatusEntry {
                local_status: LocalStatusCode::Invalid, 
                remote_status: RemoteStatusCode::NotExists,
                tracked: Some(false),
                remote_service: None,
                cols: Some(cols) };
            rows.push(entry);
        }
        print_status(rows, None);
        Ok(())
    } */


    pub fn add(&mut self, files: &Vec<String>) -> Result<()> {
        let mut num_added = 0;
        for filepath in files {
            let filename = self.relative_path_string(Path::new(&filepath.clone()))?;
            let data_file = DataFile::new(filename.clone(), self.path_context())?;
            info!("Adding file '{}'.", filename);
            self.data.register(data_file)?;
            num_added += 1;
        }
        println!("Added {} files.", num_added);
        self.save()
    }

    pub fn update(&mut self, filepath: Option<&String>) -> Result<()> {
        let path_context = self.path_context();
        self.data.update(filepath, path_context)?;
        self.save()
    }

    pub async fn link(&mut self, dir: &String, service: &String, 
                      key: &String, name: &Option<String>) -> Result<()> {
        // (0) get the relative directory path
        let dir = self.relative_path_string(Path::new(dir))?;

        // (1) save the auth key to home dir
        let mut auth_keys = AuthKeys::new();
        auth_keys.add(service, key);

        // (2) create a new remote, with a name
        // Associate a project (either by creating it, or finding it on FigShare)
        let name = match name {
            None => self.name(),
            Some(n) => n.to_string()
        };

        let service = service.to_lowercase();
        let mut remote = match service.as_str() {
            "figshare" => Ok(Remote::FigShareAPI(FigShareAPI::new(name)?)),
            _ => Err(anyhow!("Service '{}' is not supported!", service))
        }?;

        // (3) authenticate remote
        authenticate_remote(&mut remote)?;

        // (4) get the project ID
        remote.set_project().await?;

        // (4) register the remote in the manifest
        self.data.register_remote(&dir, remote)?;
        self.save()
    }

    pub async fn ls(&mut self) -> Result<()> {
        let all_remote_files = self.data.merge(true).await?;
        for (directory, remote_files) in all_remote_files.iter() {
            println!("Remote: {}", directory);
            for file in remote_files.values() {
                println!(" - {:?}", file);
            }
        }
        Ok(())
    }

    pub fn untrack(&mut self, filepath: &String) -> Result<()> {
        let filepath = self.relative_path_string(Path::new(filepath))?;
        self.data.untrack_file(&filepath)?;
        self.save()
    }

    pub fn track(&mut self, filepath: &String) -> Result<()> {
        let filepath = self.relative_path_string(Path::new(filepath))?;
        self.data.track_file(&filepath)?;
        self.save()
    }

    pub async fn pull(&mut self, dirs: &Vec<PathBuf>, overwrite: bool) -> Result<()> {
        let path_context = self.path_context();
        // TODO before any pull, we need to make sure that the project
        // status is "clean" e.g. nothing out of data.
        self.data.authenticate_remotes()?;

        //let files = remote.get_files().await?;

        let mut num_downloaded = 0;
        for (dir, remote) in self.data.remotes.iter() {

        }
 
        println!("Downloaded {} files.", num_downloaded);
        Ok(())
    }

    pub async fn push(&mut self) -> Result<()> {
        let path_context = self.path_context();
        // TODO before any push, we need to make sure that the project
        // status is "clean" e.g. nothing out of data.
        for remote in self.data.remotes.values_mut() {
            authenticate_remote(remote)?;
        }

        let data_dirs = self.data.get_files_by_directory()?;

        let mut num_uploaded = 0;
        for (dir, remote) in self.data.remotes.iter() {
            let existing_files = remote.get_files_hashmap().await?;
            info!("existing files: {:?}", existing_files);
            if let Some(data_files) = data_dirs.get(dir) {
                for data_file in data_files {
                    if !data_file.tracked {
                        info!("file {} not tracked, skipping", data_file.path);
                        continue;
                    }

                    // should we do the upload? 
                    let file_name = data_file.basename()?.clone();
                    let do_upload: bool = match existing_files.get(&file_name) {
                        None => {
                            info!("file '{}' not found on remote {}", data_file.path, remote.name());
                            true
                        }, // file does not exist on remote, upload.
                        Some(existing_remote) => {
                            match existing_remote.get_md5() {
                                // edge case: the MD5 is not yet in the remote.
                                None => return Err(anyhow!("Internal Error: MD5 not found in remote. Please report.")),

                                // check MD5s; if different, then upload.
                                Some(md5) if md5 == data_file.md5 => {
                                    print_info!("file '{}' is not being uploaded because it exists \
                                          on the remote and the MD5s match.", data_file.path);
                                    false
                                },

                                // MD5s don't match, upload.
                                Some(_) => true,
                            }
                        }
                    };

                    if do_upload {
                        print_info!("uploading file '{:?}' to {:}", data_file.path, remote.name());
                        remote.upload(&data_file, &path_context).await?;
                        num_uploaded += 1;
                    }
                }
            }
        }
        println!("Uploaded {} files.", num_uploaded);
        Ok(())
    }
}

