use url::Url;
use std::fs::File;
use std::path::{PathBuf};
use std::io::{Read,Seek,SeekFrom};
use anyhow::{anyhow,Result};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::collections::{HashSet,HashMap};
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;
use reqwest::{Method, header::{HeaderMap, HeaderValue}};
use reqwest::{Client, Response};
use colored::Colorize;

use crate::print_warn;
use crate::data::DataFile;
use crate::remote::{AuthKeys, RemoteFile};

const FIGSHARE_API_URL: &str = "https://api.figshare.com/v2/";

enum RequestData<T: serde::Serialize> {
    Json(T),
    Binary(Vec<u8>),
}

fn figshare_api_url() -> String {
    FIGSHARE_API_URL.to_string()
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FigShareAPI {
    #[serde(skip_serializing, skip_deserializing,default="figshare_api_url")]
    base_url: String,
    project_id: Option<u64>,
    name: String,
    #[serde(skip_serializing, skip_deserializing)]
    token: String
}

pub struct FigShareUpload<'a> {
    api_instance: &'a FigShareAPI,
}

/// FigShare has many upload responses that we mimic ---
/// annoyingly they are all slightly different.
/// This one is after the initial GET using the "location"
#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareUploadInfo {
    upload_token: String,
    upload_url: String,
    status: String,
    preview_state: String,
    viewer_type: String,
    is_attached_to_public_version: bool,
    id: u64,
    name: String,
    size: u64,
    is_link_only: bool,
    download_url: String,
    supplied_md5: String,
    computed_md5: String,
}

/// The response from GETs to /account/articles/{article_id}/files
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FigShareFileInfo {
    upload_token: String,
    upload_url: String,
    status: String,
    preview_state: String,
    viewer_type: String,
    is_attached_to_public_version: bool,
    id: u64,
    name: String,
    size: u64,
    is_link_only: bool,
    download_url: String,
    supplied_md5: String,
    computed_md5: String,
}

/// This struct is for response to the initial GET using the
/// upload_url. It contains more details about the actual upload.
/// Annoyingly the token is the same as upload_token, but the JSON
/// keys are different
#[derive(Debug, Serialize, Deserialize)]
pub struct FigSharePendingUploadInfo {
    token: String,
    md5: String,
    size: usize,
    name: String,
    status: String,
    parts: Vec<FigShareUploadPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FigShareUploadPart {
    part_no: u64,
    start_offset: u64,
    end_offset: u64,
    status: String,
    locked: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareCompleteUpload {
    id: u64,
    name: String,
    size: u64,
}
 

/// Manage a FigShare Upload
impl<'a> FigShareUpload<'a> {
    pub fn new(api: &'a FigShareAPI) -> Self {
        FigShareUpload { api_instance: api }
    }

    pub async fn get_article(&self, data_file: &DataFile) -> Result<Option<FigShareArticle>> {
        let remote_files = self.api_instance.get_files_hashmap().await?;
        let found_article = remote_files.values()
            .find(|article| &article.title == &data_file.path)
            .cloned();
        Ok(found_article)
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
        data.insert("title".to_string(), data_file.basename()?);
        data.insert("defined_type".to_string(), "dataset".to_string());
        debug!("creating data for article: {:?}", data);

        // (2) issue request and parse out the article ID from location
        let response = self.api_instance.issue_request(Method::POST, &url, Some(RequestData::Json(data))).await?;
        let data = response.json::<Value>().await?;
        let article_id_result = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc.split('/').last().unwrap_or_default().to_string()),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        let article_id: u64 = article_id_result?.parse::<u64>().map_err(|_| anyhow!("Failed to parse article ID"))?;
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

    async fn init_upload(&self, data_file: &DataFile, article: &FigShareArticle) -> Result<(FigShareUploadInfo, FigSharePendingUploadInfo)> {
        debug!("initializing upload of '{:?}'", data_file);
        // Requires: article ID, in FigShareArticle struct
        // (0) create URL and data
        let url = format!("account/articles/{}/files", article.id);
        let data = FigShareArticle {
            title: article.title.clone(),
            name: Some(article.title.clone()),
            id: article.id,
            url: None,
            md5: Some(data_file.md5.clone()),
            size: Some(data_file.size)
        };
        // (1) issue POST to get location
        let response = self.api_instance.issue_request(Method::POST, &url, Some(RequestData::Json(data))).await?;
        debug!("upload post response: {:?}", response);


        // (2) get location
        let data = response.json::<Value>().await?;
        let location_url = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc),
            None => Err(anyhow!("Response does not have 'location' set!"))
        }?;
        // we need to extract out the non-domain part
        let parsed_url = Url::parse(location_url)?;
        let location = parsed_url.path()
            .to_string()
            .replacen("/v2/", "/", 1);
        debug!("upload location: {:?}", location);

        // (3) issue GET to retrieve upload info
        let response = self.api_instance
            .issue_request::<HashMap<String, String>>(Method::GET, &location, None)
            .await?;
        let upload_info: FigShareUploadInfo = response.json().await?;
        debug!("upload info: {:?}", upload_info);

        // (4) Now, we need to issue another GET to initiate upload.
        // This returns the file parts info, which tells us how to split 
        // the file.
        let response = self.api_instance
            .issue_request::<HashMap<String, String>>(Method::GET, &upload_info.upload_url, None)
            .await?;
        let pending_upload_info: FigSharePendingUploadInfo = response.json().await?;
        debug!("pending upload info: {:?}", pending_upload_info);
        Ok((upload_info, pending_upload_info))
    }

    async fn upload_parts(&self, data_file: &DataFile, 
                          upload_info: &FigShareUploadInfo,
                          pending_upload_info: &FigSharePendingUploadInfo,
                          path_context: &PathBuf) -> Result<()> {
        let full_path = path_context.join(&data_file.path);
        let url = &upload_info.upload_url;
        let mut file = File::open(full_path)?;

        for part in &pending_upload_info.parts {
            let start_offset = part.start_offset;
            let end_offset = part.end_offset;

            // get the binary data between these offsets
            file.seek(SeekFrom::Start(start_offset))?;
            let mut data = vec![0u8; (end_offset - start_offset + 1) as usize];
            file.read_exact(&mut data)?;

            let part_url = format!("{}/{}", &url, part.part_no);
            let response = self.api_instance.issue_request::<HashMap<String, String>>(Method::PUT, &part_url, Some(RequestData::Binary(data)))
                .await?;
            debug!("uploaded part {} (offsets {}:{})", part.part_no, start_offset, end_offset)
        }

        Ok(())
    }

    async fn complete_upload(&self, article: &FigShareArticle, upload_info: &FigShareUploadInfo) -> Result<()> {
        let url = format!("account/articles/{}/files/{}", article.id, upload_info.id);
        let data = FigShareCompleteUpload {
            id: article.id,
            name: upload_info.name.clone(),
            size: upload_info.size
        };
        self.api_instance.issue_request(Method::POST, &url, Some(RequestData::Json(data))).await?;
        Ok(())
    }

    pub async fn upload(&self, data_file: &DataFile, path_context: &PathBuf) -> Result<()> {
        let article = self.get_or_create_article_in_project(data_file).await?.clone();
        debug!("upload() article: {:?}", article);
        let (upload_info, pending_upload_info) = self.init_upload(data_file, &article).await?;
        self.upload_parts(data_file, &upload_info, &pending_upload_info, &path_context).await?;
        self.complete_upload(&article, &upload_info).await?;
        Ok(())
    }
}

impl From<FigShareArticle> for RemoteFile {
    fn from(fgsh: FigShareArticle) -> Self {
        RemoteFile {
            name: fgsh.title,
            md5: fgsh.md5,
            size: fgsh.size,
            remote_id: Some(format!("{}", fgsh.id)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FigShareArticle {
    title: String,
    name: Option<String>,
    id: u64,
    url: Option<String>,
    md5: Option<String>,
    size: Option<u64>
}

impl FigShareArticle {
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

impl FigShareAPI {
    pub fn new(name: String) -> Result<Self> {
        let auth_keys = AuthKeys::new();
        let token = auth_keys.get("figshare".to_string())?;
        Ok(FigShareAPI { 
            base_url: FIGSHARE_API_URL.to_string(),
            project_id: None,
            name, 
            token
        })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = token;
    }

    fn set_project_id(&mut self, project_id: u64) {
        self.project_id = Some(project_id);
    }
    async fn issue_request<T: serde::Serialize>(&self, method: Method, url: &str,
                                                data: Option<RequestData<T>>) 
        -> Result<Response> {
            let mut headers = HeaderMap::new();

            let full_url = if url.starts_with("https://") || url.starts_with("http://") {
                url.to_string()
            } else {
                format!("{}{}", self.base_url, url.trim_start_matches('/'))
            };

            trace!("request URL: {:?}", full_url);

            headers.insert("Authorization", HeaderValue::from_str(&format!("token {}", self.token)).unwrap());
            trace!("headers: {:?}", headers);

            let client = Client::new();
            let response = match data {
                Some(RequestData::Json(json_data)) => client
                    .request(method, &full_url)
                    .headers(headers)
                    .json(&json_data)
                    .send()
                    .await?,

                Some(RequestData::Binary(bin_data)) => client
                    .request(method, &full_url)
                    .headers(headers)
                    .body(bin_data)
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

    pub async fn upload(&self, data_file: &DataFile, path_context: &PathBuf) -> Result<()> {
        let this_upload = FigShareUpload::new(self);
        this_upload.upload(data_file, path_context).await?;
        Ok(())
    }

    pub async fn download(&self, data_file: &DataFile, path_context: &PathBuf,
                          overwrite: bool) -> Result<()>{
        if data_file.is_alive(path_context) && !overwrite {
            return Err(anyhow!("Data file '{}' exists locally, and would be \
                               overwritten by download. Use --overwrite.", data_file.path));
        }
        Ok(())
    }

    fn ls(&self) {
    }

    /// Get all projects on this remote
    pub async fn get_projects(&self) -> Result<Vec<Value>> {
        let url = "/account/projects";
        let response = self.issue_request::<HashMap<String, String>>(Method::GET, &url, None).await?;
        trace!("reponse: {:?}", response);
        let data = response.json::<Vec<Value>>().await?;
        Ok(data)
    }

    pub async fn check_project_exists(&self) -> Result<Option<u64>> {
        let title = &self.name;
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
                Some(id) => Ok(id.as_u64()),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    pub async fn create_project(&self) -> Result<Value> {
        let title = &self.name;
        let existing_id = self.check_project_exists().await?;
        trace!("existing_id: {:?}", existing_id);
        if existing_id.is_some() {
            return Err(anyhow!("A project with the title '{}' already exists", title));
        }

        let url = "/account/projects";

        // build up the data
        let data = vec![
            ("title".to_string(), title.clone()),
        ];
        let data: HashMap<_, _> = data.into_iter().collect();

        let response = self.issue_request(Method::POST, &url, Some(RequestData::Json(data))).await?;
        trace!("response: {:?}", response);
        let data = response.json::<Value>().await?;
        info!("created remote project: {:?}", title);
        Ok(data)
    }

    pub async fn set_project(&mut self) -> Result<u64> {
        let existing_id = self.check_project_exists().await?;
        let project_id = match existing_id {
            Some(id) => {
                info!("Found an existing FigShare project (ID={:?}).", id);
                Ok(id)
            },
            None => match self.create_project().await {
                Ok(data) => match data.get("entity_id") {
                    Some(value) => match value.as_u64() {
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

    /// Get FigShare remote files as FigShareArticle
    /// (for internal FigShare stuff, main interface is through the 
    /// common RemoteFile).
    async fn get_files(&self) -> Result<Vec<FigShareArticle>> {
        let project_id = self.project_id.ok_or_else(|| anyhow!("The project ID is not set."))?;
        trace!("get_data(): project_id={:?}", project_id);
        let url = format!("/account/projects/{}/articles", project_id);

        let response = self.issue_request::<HashMap<String, String>>(Method::GET, &url, None).await?;
        let mut articles: Vec<FigShareArticle> = response.json().await?;

        // now we need to add in the supplementary FigShareArticle info
        for article in articles.iter_mut() {
            self.set_article_info(article).await?;
        }

        if check_for_duplicate_article_titles(&articles).len() > 0 {
            print_warn!("FigShare has multiple files with the \
                           same name (as different 'articles'). This can lead \
                           to problems, and these should be removed manually \
                           on FigShare.com.");
        }
        Ok(articles)
        }

    pub async fn get_remote_files(&self) -> Result<Vec<RemoteFile>> {
        let articles = self.get_files().await?;
        let remote_files = articles.into_iter().map(RemoteFile::from).collect();
        Ok(remote_files)
    }

    /// Given a list of local files, find them on the remote and download them
    /// all.
    //pub async fn download(&self, data_files: Vec<DataFile>) {
    //}

    // This returns the unique FigShareArticles
    // Warning: There can be clashing here!
    pub async fn get_files_hashmap(&self) -> Result<HashMap<String,FigShareArticle>> {
        let mut articles: Vec<FigShareArticle> = self.get_files().await?;
        let mut article_hash: HashMap<String,FigShareArticle> = HashMap::new();
        for article in articles.iter_mut() {
            self.set_article_info(article).await?;
            article_hash.insert(article.title.clone(), article.clone());
        }
        Ok(article_hash)
    }

    /// Get auxiliary data about the article
    pub async fn get_article_info(&self, article: &FigShareArticle) -> Result<FigShareFileInfo> {
        let url = format!("/account/articles/{}/files", article.id);
        let response = self.issue_request::<HashMap<String,String>>(Method::GET, &url, None).await?;
        let files_info: Vec<FigShareFileInfo> = response.json().await?;
        if files_info.len() == 1 {
            Ok(files_info[0].clone())
        } else {
            Err(anyhow!("FigShare article (ID={}) has multiple files ({} total) associated with it; \
                        this is not presently supported", &article.id, files_info.len()))
        }
    }

    pub async fn set_article_info(&self, article: &mut FigShareArticle) -> Result<()> {
        let info = self.get_article_info(&article).await?;
        article.set_md5(info.computed_md5);
        article.set_size(info.size);
        Ok(())
    }

    pub async fn track(&self) -> Result<()> {
        Ok(())
    }
}


fn check_for_duplicate_article_titles(articles: &Vec<FigShareArticle>) -> HashSet<String> {
    let mut titles = HashSet::new();
    let mut duplicates = HashSet::new();
    
    for article in articles {
        if !titles.insert(article.title.clone()) {
            duplicates.insert(article.title.clone());
        }
    }

    duplicates
}
