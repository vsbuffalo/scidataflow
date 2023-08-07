use serde_yaml;
use std::{fs, hash::Hash};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::env;
use anyhow::{anyhow,Result};
use log::{info, trace, debug};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;
use reqwest::{Method, header::{HeaderMap, HeaderValue, AUTHORIZATION}};
use reqwest::{Client, Response, Error };
use reqwest::{StatusCode};
use tokio;

use crate::data::DataFile;
use crate::project;
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

pub type ResponseResult = Result<Value>;
pub type ResponseResults = Result<Vec<Value>>;

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
    pub async fn get_projects(&self) -> ResponseResults {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_projects().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    pub async fn create_project(&self, dir: &String) -> ResponseResult {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.create_project(dir).await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

    pub async fn set_project(&mut self, dir: &String) -> Result<i64> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.set_project(dir).await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

   pub async fn get_files(&mut self) -> Result<Vec<FigShareArticle>> {
        match self {
            Remote::FigShareAPI(figshare_api) => figshare_api.get_files().await,
            Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
        }
    }

   pub async fn track(&mut self) -> Result<(),String> {
       Ok(())
   }

}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareAPI {
    base_url: String,
    project_id: Option<i64>,

    #[serde(skip_serializing, skip_deserializing)]
    token: String
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareUpload {
    api_instance: FigShareAPI,
}

/// Manage a FigShare Upload
impl FigShareUpload {
    pub async fn get_article(&self, data_file: &DataFile) -> Result<Option<FigShareArticle>> {
        let remote_files = self.api_instance.get_files().await?;
        remote_files.into_iter()
            .find(|article| &article.title == &data_file.path)
            .map(|article| Ok(Some(article))).unwrap_or(Ok(None))
    }

    pub async fn create_article_in_project(&self, data_file: &DataFile) -> Result<FigShareArticle> {
        // (0) Get the project_id
        let project_id = self.api_instance.project_id;
        let url = match project_id {
            Some(id) => Ok(format!("account/projects/{}/articles", id)), // wrap it in Ok
            None => Err(anyhow!("Cannot create article in project; project ID is None"))
        }?;

        // (1) create the data for this article
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("title".to_string(), data_file.path.clone());
        data.insert("defined_type".to_string(), "dataset".to_string());

        // (2) issue request and parse out the article ID from location
        let response = self.api_instance.issue_request(Method::POST, &url, Some(data)).await?;
        let data = response.json::<Value>().await?;
        let article_id_result = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc.split('/').last().unwrap_or_default().to_string()),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        let article_id: i64 = article_id_result?.parse::<i64>().map_err(|_| anyhow!("Failed to parse article ID"))?;

        // (3) create and return the FigShareArticle
        Ok(FigShareArticle { title: data_file.path.clone(), id: article_id, url: None })
    }

    pub async fn get_or_create_article_in_project(&self, data_file: &DataFile) -> Result<FigShareArticle> {
        let article = self.get_article(data_file).await?;
        match article {
            Some(article) => Ok(article),
            None => self.create_article_in_project(data_file).await
        }
    }

    async fn init_upload(&self, data_file: &DataFile, article: FigShareArticle) -> Result<()> {
        // Once we have an article ID, we need to initialize the upload
        let url = format!("account/articles/{}/files", article.id);
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("name".to_string(), article.title);
        data.insert("md5".to_string(), data_file.md5.clone());
        data.insert("size".to_string(), format!("{}", data_file.size));
        let response = self.api_instance.issue_request(Method::POST, &url, Some(data)).await?;

        let data = response.json::<Value>().await?;
        let article_id_result = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc.split('/').last().unwrap_or_default().to_string()),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        let article_id: i64 = article_id_result?.parse::<i64>().map_err(|_| anyhow!("Failed to parse article ID"))?;

        Ok(())
    }

    pub fn upload(&self, data_file: &DataFile) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareArticle {
    title: String,
    id: i64,
    url: Option<String>,
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
        -> Result<Response> {
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
                    .await?,
                None => client.request(method, &full_url)
                    .headers(headers)
                    .send()
                    .await?,
            };

            let response_status = response.status();
            if response_status.is_success() {
                Ok(response)
            } else {
                Err(anyhow!("HTTP Error: {}", response_status))
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
        let response = self.issue_request(Method::GET, &url, None).await?;
        debug!("reponse: {:?}", response);
        let data = response.json::<Vec<Value>>().await?;
        Ok(data)
    }

    pub async fn check_project_exists(&self, title: &String) -> Result<Option<i64>> {
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
            return Err(anyhow!("A project with the title '{}' already exists", title));
        }

        let url = "/account/projects";

        // build up the data
        let data = vec![
            ("title".to_string(), title.clone()),
        ];
        let data: HashMap<_, _> = data.into_iter().collect();

        let response = self.issue_request(Method::POST, &url, Some(data)).await?;
        debug!("response: {:?}", response);
        let data = response.json::<Value>().await?;
        info!("created remote project: {:?}", title);
        Ok(data)
    }

    pub async fn set_project(&mut self, title: &String) -> Result<i64> {
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
                        None => Err(anyhow!("Entity id is not an integer")),
                    },
                    None => Err(anyhow!("Entity id is missing")),
                },
                Err(err) => Err(anyhow!("Invalid response: {}", err)),
            },
        }?;

        self.set_project_id(project_id);
        Ok(project_id)
    }

    pub async fn get_files(&self) -> Result<Vec<FigShareArticle>> {
        let project_id = self.project_id; 
        debug!("project_id={:?}", project_id);
        let url = format!("/account/projects/{}/articles", project_id.unwrap().to_string());

        let response = self.issue_request(Method::GET, &url, None).await?;
        let files: Vec<FigShareArticle> = response.json().await?;
        Ok(files)
    }

    pub async fn track(&self) -> Result<()> {
        Ok(())
    }

}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DataDryadAPI {
    base_url: String,

    #[serde(skip_serializing)]
    token: String
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

