use serde_yaml;
use std::{fs, path};
use std::fs::File;
use std::path::Path;
use std::io::Read;
use std::env;
use anyhow::{anyhow,Result};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::collections::HashMap;
use trauma::{download::Download};
use serde_derive::{Serialize,Deserialize};
use reqwest::Url;

use crate::lib::data::{DataFile,MergedFile};
use crate::lib::api::figshare::FigShareAPI;
use crate::lib::api::dryad::DataDryadAPI;
use crate::lib::api::zenodo::ZenodoAPI;
use crate::lib::project::LocalMetadata;


const AUTHKEYS: &str = ".scidataflow_authkeys.yml";

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadInfo {
    pub url: String,
    pub path: String,
} 

impl DownloadInfo {
    pub fn trauma_download(&self) -> Result<Download> {
        Ok(Download::new(&Url::parse(&self.url)?, &self.path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteFile {
    pub name: String,
    pub md5: Option<String>,
    pub size: Option<u64>,
    pub remote_service: String,
    pub url: Option<String>
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
        md5.filter(|digest| !digest.is_empty())
    }
    pub fn set_size(&mut self, size: u64) {
        self.size = Some(size);
    }
}

#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
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

    pub fn add(&mut self, service: &str, key: &str) {
        let service = service.to_lowercase();
        self.keys.insert(service, key.to_owned());
        self.save();
    }

    pub fn temporary_add(&mut self, service: &str, key: &str) {
        // no save, i.e. for testing -- we do *not* want to overwrite the 
        // dev's own keys.
        let service = service.to_lowercase();
        self.keys.insert(service, key.to_owned());
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
    ZenodoAPI(ZenodoAPI),
}


macro_rules! service_not_implemented {
    ($service:expr) => {
        Err(anyhow!("{} not implemented yet.", $service))
    };
}

// NOTE: these are not implemented as traits because many are async, and
// it looked like this wasn't implemented yet.
impl Remote {
    pub fn name(&self) -> &str {
        match self {
            Remote::FigShareAPI(_) => "FigShare",
            Remote::DataDryadAPI(_) => "Dryad",
            Remote::ZenodoAPI(_) => "Zenodo"
        }
    }
    // initialize the remote (i.e. tell it we have a new empty data set)
    pub async fn remote_init(&mut self, local_metadata: LocalMetadata) -> Result<()> {
        match self {
            Remote::FigShareAPI(fgsh_api) => fgsh_api.remote_init(local_metadata).await,
            Remote::ZenodoAPI(znd_api) => znd_api.remote_init(local_metadata).await,
            Remote::DataDryadAPI(_) => service_not_implemented!("DataDryad"),
        }
    }
    pub async fn get_files(&self) -> Result<Vec<RemoteFile>> {
        match self {
            Remote::FigShareAPI(fgsh_api) => fgsh_api.get_remote_files().await,
            Remote::ZenodoAPI(znd_api) => znd_api.get_remote_files().await,
            Remote::DataDryadAPI(_) => service_not_implemented!("DataDryad"),
        }
    }
    pub async fn get_files_hashmap(&self) -> Result<HashMap<String,RemoteFile>> {
        // now we can use the common interface! :)
        let remote_files = self.get_files().await?;
        let mut file_map: HashMap<String,RemoteFile> = HashMap::new();
        for file in remote_files.into_iter() {
            file_map.insert(file.name.clone(), file.clone());
        }
        Ok(file_map)
    }
    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<bool> {
        match self {
            Remote::FigShareAPI(fgsh_api) => fgsh_api.upload(data_file, path_context, overwrite).await,
            Remote::ZenodoAPI(znd_api) => znd_api.upload(data_file, path_context, overwrite).await,
            Remote::DataDryadAPI(_) => service_not_implemented!("DataDryad"),
        }
    }
    // Get Download info: the URL (with token) and destination
    // TODO: could be struct, if some APIs require more authentication
    // Note: requires each API actually *check* overwrite.
    pub fn get_download_info(&self, merged_file: &MergedFile, path_context: &Path, overwrite: bool) -> Result<DownloadInfo> {
        match self {
            Remote::FigShareAPI(fgsh_api) => fgsh_api.get_download_info(merged_file, path_context, overwrite),
            Remote::ZenodoAPI(_) => Err(anyhow!("ZenodoAPI does not support get_project method")),
            Remote::DataDryadAPI(_) => service_not_implemented!("DataDryad"),
        }
    }
}

pub fn authenticate_remote(remote: &mut Remote) -> Result<()> {
    // Get the keys off disk
    let auth_keys = AuthKeys::new();
    let error_message = |service_name: &str, token_name: &str| {
        format!("Expected {} access token not found.\n\n\
                If you used 'sdf link', it should have saved this token in ~/.scidataflow_authkeys.yml.\n\
                You will need to re-add this key manually, by adding a line to this file like:\n\
                {}: <TOKEN>", service_name, token_name)
    };

    match remote {
        Remote::FigShareAPI(ref mut fgsh_api) => {
            let token = auth_keys.keys.get("figshare").cloned()
                .ok_or_else(|| anyhow::anyhow!(error_message("FigShare", "figshare")))?;
            fgsh_api.set_token(token);
        },
        Remote::ZenodoAPI(ref mut znd_api) => {
            let token = auth_keys.keys.get("zenodo").cloned()
                .ok_or_else(|| anyhow::anyhow!(error_message("Zenodo", "zenodo")))?;
            znd_api.set_token(token);
        },
        // handle other Remote variants as necessary
        _ => Err(anyhow!("Could not find correct API in authenticate_remote()"))?
    }
    Ok(())
}


// Common enum for issue_request() methods of APIs
#[derive(Debug)]
pub enum RequestData<T: serde::Serialize> {
    Json(T),
    Binary(Vec<u8>),
    File(tokio::fs::File),
    Empty
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

