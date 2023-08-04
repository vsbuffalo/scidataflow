use serde_yaml;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::env;
use log::{info, trace, debug};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use reqwest::{Method, header::{HeaderMap, HeaderValue, AUTHORIZATION}};
use reqwest::{Client, Response, Error };
use reqwest::{StatusCode};
use tokio;

use crate::tokio::io::ErrorKind;
use super::data::DataCollection;

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
    pub async fn get_project(&self) -> Result<String, String> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_project().await,
            Remote::DataDryadAPI(_) => Err("DataDryadAPI does not support get_project method".to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareAPI {
    base_url: String,

    #[serde(skip_serializing, skip_deserializing)]
    token: String
}

#[derive(Debug, Deserialize, Serialize)]
struct Project {
    storage: String,
    role: String,
    id: u32,
    title: String,
    url: String,
    created_date: String,
    published_date: Option<String>,
    modified_date: String,
}

impl FigShareAPI {
    pub fn new() -> Self {
        let auth_keys = AuthKeys::new();
        let token = auth_keys.keys.get("figshare").cloned().unwrap_or_default();
        FigShareAPI { 
            base_url: "https://api.figshare.com/v2/".to_string(),
            token: token
        }
    }

    fn set_token(&mut self, token: String) {
        self.token = token;
    }

    async fn issue_request(&self, method: Method, url: &str, data: Option<HashMap<String, String>>) 
        -> Result<Response, String> {
            let mut headers = HeaderMap::new();
            let url = url.trim_start_matches('/');
            let full_url = format!("{}{}", self.base_url, url);

            debug!("request URL: {:?}", full_url);

            headers.insert("Authorization", HeaderValue::from_str(&format!("token {}", self.token)).unwrap());
            debug!("headers: {:?}", headers);
            debug!("data: {:?}", data);

            let client = Client::new();
            let response = match data {
                Some(data) => client
                    .request(method, &full_url)
                    .headers(headers)
                    .json(&data)
                    .send()
                    .await.map_err(|e| format!("request error: {:?}", e))?,
                None => client.request(method, &full_url)
                    .headers(headers)
                    .send()
                    .await.map_err(|e| format!("no data error: {:?}", e))?,
            };

            let response_status = response.status();
            if response_status.is_success() {
                Ok(response)
            } else {
                Err(format!("HTTP Error: {}", response_status))
            }
        }

    fn upload(&self) {
    }
    fn download(&self) {
    }
    fn ls(&self) {
    }
    pub async fn get_project(&self) -> Result<String, String> {
        let url = "/account/projects";
        let response = match self.issue_request(Method::GET, &url, None).await {
            Ok(response) => response,
            Err(err) => {
                eprintln!("Error while fetching project: {}", err);
                return Err(err.to_string());
            }
        };
        info!("reponse: {:?}", response);
        let data = response.json::<Vec<Project>>()
            .await
            .map_err(|e| format!("json error: {:?}", e))?;
        Ok(format!("{:?}", data))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DataDryadAPI {
    base_url: String,

    #[serde(skip_serializing)]
    token: String
}


pub fn initialize_remotes(data_collection: &mut DataCollection) -> Result<(), String> {
    let auth_keys = AuthKeys::new();

    for remote in data_collection.remotes.values_mut() {
        match remote {
            Remote::FigShareAPI(ref mut figshare_api) => {
                let token = auth_keys.keys.get("figshare").cloned()
                    .ok_or("Expected figshare key not found")?;
                figshare_api.set_token(token);
            },
            // handle other Remote variants as necessary
            _ => {},
        }
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

