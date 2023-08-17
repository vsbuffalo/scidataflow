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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteFile {
    pub name: String,
    pub md5: Option<String>,
    pub size: Option<u64>,
    pub remote_service: String
}


// This is the status of the local state with the remote state.
// There are huge number of combinations between tracked, untracked
// local files, and whether the manifest and file MD5s agree or 
// disagree (a "messy" state). However, it's better to handle few
// cases well, and error out.
//
// Note that these states are independent of whether something is
// tracked. Tracking only indicates whether the next pull/push 
// should sync the file (if tracked). Tracked files are *always* 
// in the manifest (since that is where that state is stored).
//
// NoLocal files will *not* be sync'd, since they are no in the 
// manifest, and sciflow will only get pull/push things in the 
// manifest.
//
// Clean state: everything on the manifest tracked by the remote is
// local, with nothing else.
#[derive(Debug,PartialEq,Clone)]
pub enum RemoteStatusCode {
    Current,              // local and remote files are identical
    MessyLocal,           // local file is different than remote and manifest, which agree
    Different,            // the local file is current, but different than the remote
    NotExists,            // no remote file
    Exists,               // remote file exists, but remote does not support MD5s
    NoLocal,              // a file on the remote, but not in manifest or found locally
    DeletedLocal,         // a file on the remote and in manifest, but not found locally
    //OutsideSource,        // a file on the remote, but not in manifest but *is* found locally
    Invalid
}

impl RemoteFile {
    pub fn set_md5(&mut self, md5: String) {
        self.md5 = Some(md5);
    }
    pub fn get_md5(&self) -> Option<String> {
        let md5 = self.md5.clone();
        match md5 {
            Some(digest) => {
                if digest.len() > 0 { Some(digest) } else { None }
            },
            None => None
        }
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
    // initialize the remote (i.e. tell it we have a new empty data set)
    pub async fn remote_init(&mut self) -> Result<()> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.remote_init().await,
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

    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<()> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.upload(data_file, path_context, overwrite).await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }
    pub async fn download(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<()> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.download(data_file, path_context, overwrite).await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }



    //pub async fn pull(&self, path_context: &PathBuf, overwrite: bool) -> Result<()> {
    //    match self {
    //        Remote::FigShareAPI(figshare_api) => figshare_api.download(path_context, overwrite).await,
    //        Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
    //    }
    //}


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

