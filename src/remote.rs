use serde_yaml;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::env;
use std::collections::HashMap;
use reqwest::Error;
use serde_derive::{Serialize,Deserialize};
use tokio;

use crate::traits::RemoteAPI;

const AUTHKEYS: &str = ".sciflow_authkeys.yml";

#[derive(Serialize, Deserialize, Debug)]
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
        AuthKeys { keys }
    }

    pub fn add(&mut self, service: &String, key: &String) {
        let service = service.to_lowercase();
        self.keys.insert(service, key.clone());
        self.save();
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

impl Remote {
    pub fn name(&self) -> &str {
        match self {
            Remote::FigShareAPI(_) => "FigShare",
            Remote::DataDryadAPI(_) => "Dryad",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareAPI {
    base_url: String,
}

impl FigShareAPI {
    pub fn new() -> Self {
        FigShareAPI { 
            base_url: "https://api.figshare.com/v2/".to_string(),
        }
    }
}

impl RemoteAPI for FigShareAPI {
    fn upload(&self) {
    }
    fn download(&self) {
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DataDryadAPI;
impl RemoteAPI for DataDryadAPI {
    fn upload(&self) {
    }
    fn download(&self) {
    }
}


