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

use crate::data::DataFile;
use crate::figshare::{FigShareAPI,FigShareArticle};
use crate::dryad::{DataDryadAPI};

const AUTHKEYS: &str = ".sciflow_authkeys.yml";

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

   pub async fn get_files(&self) -> Result<Vec<FigShareArticle>> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_files().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

   pub async fn get_files_hashmap(&self) -> Result<HashMap<String,FigShareArticle>> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_files_hashmap().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
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

