use serde_yaml;
use serde;
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

   pub async fn upload(&self, data_file: &DataFile) -> Result<()> {
       match self {
           Remote::FigShareAPI(figshare_api) => figshare_api.upload(data_file).await,
           Remote::DataDryadAPI(_) => Err(anyhow!("DataDryadAPI does not support get_project method")),
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

pub struct FigShareUpload<'a> {
    api_instance: &'a FigShareAPI,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareUploadInfo {
    upload_token: String,
    upload_url: String,
    status: String,
    preview_state: String,
    viewer_type: String,
    is_attached_to_public_version: bool,
    id: i64,
    name: String,
    size: i64,
    is_link_only: bool,
    download_url: String,
    supplied_md5: String,
    computed_md5: String,
}

/// Manage a FigShare Upload
impl<'a> FigShareUpload<'a> {
    pub fn new(api: &'a FigShareAPI) -> Self {
        FigShareUpload { api_instance: api }
    }

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
            Some(id) => Ok(format!("account/projects/{}/articles", id)),
            None => Err(anyhow!("Cannot create article in project; project ID is None"))
        }?;

        // (1) create the data for this article
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("title".to_string(), data_file.path.clone());
        data.insert("defined_type".to_string(), "dataset".to_string());
        debug!("creating data for article: {:?}", data);

        // (2) issue request and parse out the article ID from location
        let response = self.api_instance.issue_request(Method::POST, &url, Some(data)).await?;
        let data = response.json::<Value>().await?;
        let article_id_result = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc.split('/').last().unwrap_or_default().to_string()),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        let article_id: i64 = article_id_result?.parse::<i64>().map_err(|_| anyhow!("Failed to parse article ID"))?;
        debug!("got article ID: {:?}", article_id);

        // (3) create and return the FigShareArticle
        Ok(FigShareArticle {
            title: data_file.path.clone(),
            name: Some(data_file.path.clone()),
            id: article_id,
            url: None,
            md5: None,
            size: None
        })
    }

    pub async fn get_or_create_article_in_project(&self, data_file: &DataFile) -> Result<FigShareArticle> {
        let article = self.get_article(data_file).await?;
        match article {
            Some(article) => Ok(article),
            None => self.create_article_in_project(data_file).await
        }
    }

    async fn init_upload(&self, data_file: &DataFile, article: FigShareArticle) -> Result<FigShareUploadInfo> {
        debug!("initializing upload of '{:?}'", data_file);
        // Requires: article ID, in FigShareArticle struct
        // (0) create URL and data
        let url = format!("account/articles/{}/files", article.id);
        let data = FigShareArticle {
            title: article.title.clone(),
            name: Some(article.title),
            id: article.id,
            url: None,
            md5: Some(data_file.md5.clone()),
            size: Some(data_file.size)
        };
        // (1) issue POST 
        let response = self.api_instance.issue_request(Method::POST, &url, Some(data)).await?;
        debug!("upload post response: {:?}", response);


        // (2) get location
        let data = response.json::<Value>().await?;
        let location = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        //let article_id: i64 = article_id_result?.parse::<i64>().map_err(|_| anyhow!("Failed to parse article ID"))?;
        let response = self.api_instance
            .issue_request::<HashMap<String, String>>(Method::GET, location?, None)
            .await?;
        let upload_info: FigShareUploadInfo = response.json().await?;
        Ok(upload_info)
    }

    pub async fn upload(&self, data_file: &DataFile) -> Result<()> {
        let article = self.get_or_create_article_in_project(data_file).await?;
        debug!("upload() article: {:?}", article);
        self.init_upload(data_file, article).await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareArticle {
    title: String,
    name: Option<String>,
    id: i64,
    url: Option<String>,
    md5: Option<String>,
    size: Option<u64>
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

    async fn issue_request<T: serde::Serialize>(&self, method: Method, url: &str, 
                           data: Option<T>) 
        -> Result<Response> {
            let mut headers = HeaderMap::new();
            let url = url.trim_start_matches('/');
            let full_url = format!("{}{}", self.base_url, url);

            debug!("request URL: {:?}", full_url);

            headers.insert("Authorization", HeaderValue::from_str(&format!("token {}", self.token)).unwrap());
            debug!("headers: {:?}", headers);

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
                Err(anyhow!("HTTP Error: {}\nurl: {:?}\n{:?}", response_status, full_url, response.text().await?))
            }
        }

    pub async fn upload(&self, data_file: &DataFile) -> Result<()> {
        let this_upload = FigShareUpload::new(self);
        this_upload.upload(data_file).await?;
        Ok(())
    }
    fn download(&self) {
    }
    fn ls(&self) {
    }

    /// Get all projects on this remote
    pub async fn get_projects(&self) -> ResponseResults {
        let url = "/account/projects";
        let response = self.issue_request::<HashMap<String, String>>(Method::GET, &url, None).await?;
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
        debug!("get_files(): project_id={:?}", project_id);
        let url = format!("/account/projects/{}/articles", project_id.unwrap().to_string());

        let response = self.issue_request::<HashMap<String, String>>(Method::GET, &url, None).await?;
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

