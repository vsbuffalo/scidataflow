use serde_yaml;
use std::{fs};
use std::fs::File;
use std::path::{Path,PathBuf};
use std::io::{Read};
use std::env;
use anyhow::{anyhow,Result};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;

use crate::utils::ensure_exists;
use crate::data::{DataFile,LocalStatusCode};
use crate::figshare::{FigShareAPI,FigShareArticle};
use crate::dryad::{DataDryadAPI};

const AUTHKEYS: &str = ".sciflow_authkeys.yml";

#[derive(PartialEq,Clone)]
pub enum RemoteStatusCode {
   NotExists,
   Current,
   MD5Mismatch,
   NoMD5,
   Invalid
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteFile {
    pub name: String,
    pub md5: Option<String>,
    pub size: Option<u64>,
    pub remote_id: Option<String>
}

impl RemoteFile {
    pub fn set_md5(&mut self, md5: String) {
        self.md5 = Some(md5);
    }
    pub fn get_md5(&self) -> Option<String> {
        self.md5.clone()
    }
    pub fn set_size(&mut self, size: u64) {
        self.size = Some(size);
    }
}

#[derive(Serialize, Deserialize,  PartialEq, Debug)]
pub struct AuthKeys {
    keys: HashMap<String,String>
}

impl AuthKeys {
    pub fn new() -> Self {
        let home_dir = env::var("HOME")
            .expect("Could not infer home directory");
        let path = Path::new(&home_dir).join(AUTHKEYS);
        let keys = match path.exists() {
            true => {
                let mut contents = String::new();
                File::open(path)
                    .unwrap()
                    .read_to_string(&mut contents)
                    .unwrap();
                serde_yaml::from_str(&contents)
                    .unwrap_or_else(|_| panic!("Cannot load {}!", AUTHKEYS))
            }, 
            false => {
                let keys: HashMap<String,String> = HashMap::new();
                keys
            }
        };
        debug!("auth_keys: {:?}", keys);
        AuthKeys { keys }
    }

    pub fn add(&mut self, service: &String, key: &String) {
        let service = service.to_lowercase();
        self.keys.insert(service, key.clone());
        self.save();
    }

    pub fn get(&self, service: String) -> Result<String> {
        match self.keys.get(&service) {
            None => Err(anyhow!("no key found for service '{}'", service)),
            Some(key) => Ok(key.to_string())
        }
    }

    pub fn save(&self) {
        let serialized_keys = serde_yaml::to_string(&self.keys)
            .expect("Cannot serialize authentication keys!");
        let home_dir = env::var("HOME")
            .expect("Could not infer home directory");
        let path = Path::new(&home_dir).join(AUTHKEYS);
        fs::write(path, serialized_keys)
            .unwrap_or_else(|_| panic!("Cound not write {}!", AUTHKEYS));
    }
}


#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Remote {
    FigShareAPI(FigShareAPI),
    DataDryadAPI(DataDryadAPI),
}

// NOTE: these are not implemented as traits because many are async, and
// it looked like this wasn't implemented yet.
impl Remote {
    pub fn name(&self) -> &str {
        match self {
            Remote::FigShareAPI(_) => "FigShare",
            Remote::DataDryadAPI(_) => "Dryad",
        }
    }
    pub async fn get_projects(&self) -> Result<Vec<Value>> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_projects().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    pub async fn set_project(&mut self) -> Result<u64> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.set_project().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    pub async fn get_files(&self) -> Result<Vec<RemoteFile>> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_remote_files().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    pub async fn get_files_hashmap(&self) -> Result<HashMap<String,RemoteFile>> {
        // now we can use the common interface!
        let remote_files = self.get_files().await?;
        let mut file_map: HashMap<String,RemoteFile> = HashMap::new();
        for file in remote_files.into_iter() {
            file_map.insert(file.name.clone(), file.clone());
        }
        Ok(file_map)
    }

    pub async fn track(&mut self) -> Result<(),String> {
        Ok(())
    }

    pub async fn upload(&self, data_file: &DataFile, path_context: &PathBuf) -> Result<()> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.upload(data_file, path_context).await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    //pub async fn pull(&self, path_context: &PathBuf, overwrite: bool) -> Result<()> {
    //    match self {
    //        Remote::FigShareAPI(figshare_api) => figshare_api.download(path_context, overwrite).await,
    //        Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
    //    }
    //}

    pub async fn file_status(&self, data_file: &DataFile, path_context: &PathBuf) -> Result<(String, RemoteStatusCode)> {
        if !data_file.is_alive(path_context) {
            return Err(anyhow!("Data file '{}' no longer exists!", data_file.path));
        }
        let path = data_file.full_path(path_context)?;
        let file_name = path.file_name()
            .ok_or(anyhow!("Invalid path: {:?}", path))?
            .to_string_lossy()
            .to_string();

        let remote_files = self.get_files().await?;
        let service = self.name().to_string();

        // Check if the DataFile is clean
        if data_file.tracked && data_file.status(path_context)?.local_status != LocalStatusCode::Current {
            // Not comparing as it's "unclean"
            return Err(anyhow!("The data file '{}' is tracked by a remote and \
                               has changed locally; its MD5 hash in the data \
                               manifest differs from its true MD5.\nPlease run: \
                               swf update.", data_file.path));
        }

        // Do we have a remote file that matches this DataFile's name?
        if let Some(remote_file) = remote_files.iter().find(|f| f.name == file_name) {
            // Let's now compare MD5s
            if let Some(remote_md5) = &remote_file.md5 {
                if let Ok(Some(local_md5)) = data_file.get_md5(&path_context) {
                    if local_md5 == *remote_md5 {
                        Ok((service, RemoteStatusCode::Current))
                    } else {
                        Ok((service, RemoteStatusCode::MD5Mismatch))
                    }
                } else {
                    return Err(anyhow!("Cannot get MD5 of data file '{}'. Is it tracked?", data_file.path))
                }
            } else {
                Ok((service, RemoteStatusCode::NoMD5))
            }
        } else {
            Ok((service, RemoteStatusCode::NotExists))
        }

    }
}

pub fn authenticate_remote(remote: &mut Remote) -> Result<()> {
    // Get they keys off disk
    let auth_keys = AuthKeys::new();
    match remote {
        Remote::FigShareAPI(ref mut figshare_api) => {
            let token = auth_keys.keys.get("figshare").cloned()
                .ok_or_else(|| anyhow::anyhow!("Expected figshare key not found"))?;
            figshare_api.set_token(token);
        },
        // handle other Remote variants as necessary
        _ => {},
    }
    Ok(())
}


/* impl DataDryadAPI {
   fn upload(&self) {
   }
   fn download(&self) {
   }
   fn ls(&self) {
   }
   fn get_project(&self) -> Result<String, String> {
   Ok("ID".to_string())        
   }
   }
   */

