use serde_yaml;
use std::{fs, hash::Hash};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::env;
use log::{info, trace, debug};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;
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

pub type ResponseResult = Result<Value, String>;
pub type ResponseResults = Result<Vec<Value>, String>;

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
    pub async fn get_projects(&self) -> ResponseResults {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_projects().await,
            Remote::DataDryadAPI(_) => Err("DataDryadAPI does not support get_project method".to_string()),
        }
    }

    pub async fn create_project(&self, dir: &String) -> ResponseResult {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.create_project(dir).await,
            Remote::DataDryadAPI(_) => Err("DataDryadAPI does not support get_project method".to_string()),
        }
    }

    pub async fn set_project(&mut self, dir: &String) -> Result<i64,String> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.set_project(dir).await,
            Remote::DataDryadAPI(_) => Err("DataDryadAPI does not support get_project method".to_string()),
        }
    }


}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareAPI {
    base_url: String,
    project_id: Option<i64>,

    #[serde(skip_serializing, skip_deserializing)]
    token: String
}


impl FigShareAPI {
    pub fn new() -> Self {
        let auth_keys = AuthKeys::new();
        let token = auth_keys.keys.get("figshare").cloned().unwrap_or_default();
        FigShareAPI { 
            base_url: "https://api.figshare.com/v2/".to_string(),
            project_id: None,
            token: token
        }
    }

    fn set_token(&mut self, token: String) {
        self.token = token;
    }

    fn set_project_id(&mut self, project_id: i64) {
        self.project_id = Some(project_id);
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
    
    /// Get all projects on this remote
    pub async fn get_projects(&self) -> ResponseResults {
        let url = "/account/projects";
        let response = match self.issue_request(Method::GET, &url, None).await {
            Ok(response) => response,
            Err(err) => {
                eprintln!("Error while fetching project: {}", err);
                return Err(err.to_string());
            }
        };
        debug!("reponse: {:?}", response);
        let data = response.json::<Vec<Value>>()
            .await
            .map_err(|e| format!("json error: {:?}", e))?;
        Ok(data)
    }

    pub async fn check_project_exists(&self, title: &String) -> Result<Option<i64>, String> {
        let projects = self.get_projects().await?;
        //info!("PROJECTS: {:?}", projects);
        let project = projects.iter().find(|project| {
            match project.get("title") {
                Some(value) => {
                    if let Some(title_value) = value.as_str() {
                        title_value == title.as_str()
                    } else {
                        false
                    }
                },
                None => false,
            }
        });

        match project {
            Some(project) => match project.get("id") {
                Some(id) => Ok(id.as_i64()),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    pub async fn create_project(&self, title: &String) -> ResponseResult {
        let existing_id = self.check_project_exists(title).await?;
        debug!("existing_id: {:?}", existing_id);
        if existing_id.is_some() {
            return Err(format!("A project with the title '{}' already exists", title));
        }

        let url = "/account/projects";

        // build up the data
        let data = vec![
            ("title".to_string(), title.clone()),
        ];
        let data: HashMap<_, _> = data.into_iter().collect();

        let response = match self.issue_request(Method::POST, &url, Some(data)).await {
            Ok(response) => response,
            Err(err) => {
                eprintln!("Error while creating project: {}", err);
                return Err(err.to_string());
            }
        };
        debug!("response: {:?}", response);
        let data = response.json::<Value>()
            .await
            .map_err(|e| format!("json error: {:?}", e))?;
        info!("created remote project: {:?}", title);
        Ok(data)
    }

    pub async fn set_project(&mut self, title: &String) -> Result<i64, String> {
        let existing_id = self.check_project_exists(title).await?;
        let project_id = match existing_id {
            Some(id) => {
                debug!("set_project() found an existing project (ID={:?})", id);
                Ok(id)
            },
            None => match self.create_project(title).await {
                Ok(data) => match data.get("entity_id") {
                    Some(value) => match value.as_i64() {
                        Some(id) => Ok(id),
                        None => Err("Entity id is not an integer".to_string()),
                    },
                    None => Err("Entity id is missing".to_string()),
                },
                Err(err) => Err(format!("Invalid response: {}", err)),
            },
        }?;

        self.set_project_id(project_id);
        Ok(project_id)
    }

}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DataDryadAPI {
    base_url: String,

    #[serde(skip_serializing)]
    token: String
}

pub fn initialize_remote(remote: &mut Remote) -> Result<(), String> {
    // Get they keys off disk
    let auth_keys = AuthKeys::new();
    match remote {
        Remote::FigShareAPI(ref mut figshare_api) => {
            let token = auth_keys.keys.get("figshare").cloned()
                .ok_or("Expected figshare key not found")?;
            figshare_api.set_token(token);
        },
        // handle other Remote variants as necessary
        _ => {},
    }
    Ok(())
}

pub fn initialize_remotes(data_collection: &mut DataCollection) -> Result<(), String> {
    for remote in data_collection.remotes.values_mut() {
        initialize_remote(remote)?;
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

