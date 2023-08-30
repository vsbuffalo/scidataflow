// FigShare API
//
// Notes:
// FigShare's API design is, in my view, a bit awkward. 
// There are articles, files, and projects. 

use url::Url;
use std::fs;
use std::path::{Path,PathBuf};
use std::io::{Read,Seek,SeekFrom};
use anyhow::{anyhow,Result};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;
use reqwest::{Method, header::{HeaderMap, HeaderValue}};
use reqwest::{Client, Response, Body};
use colored::Colorize;
use futures_util::StreamExt;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[allow(unused_imports)]
use crate::{print_info,print_warn};
use crate::lib::data::{DataFile, MergedFile};
use crate::lib::remote::{AuthKeys, RemoteFile, DownloadInfo,RequestData};
use crate::lib::project::LocalMetadata;

use super::zenodo::ZenodoDeposition;

pub const FIGSHARE_BASE_URL: &str = "https://api.figshare.com/v2/";

// for testing:
const TEST_TOKEN: &str = "test-token";

// for serde deserialize default
fn figshare_api_url() -> String {
    FIGSHARE_BASE_URL.to_string()
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FigShareAPI {
    #[serde(skip_serializing, skip_deserializing,default="figshare_api_url")]
    base_url: String,
    // one remote corresponds to a FigShare article
    article_id: Option<u64>,
    name: String,
    #[serde(skip_serializing, skip_deserializing)]
    token: String
}

pub struct FigShareUpload<'a> {
    api_instance: &'a FigShareAPI,
}

/// The response from GETs to /account/articles/{article_id}/files
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FigShareFile {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct FigShareNewUpload {
    md5: String,
    name: String,
    size: u64
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

   async fn init_upload(&self, data_file: &DataFile) -> Result<(FigShareFile, FigSharePendingUploadInfo)> {
        debug!("initializing upload of '{:?}'", data_file);
        // Requires: article ID, in FigShareArticle struct
        // (0) create URL and data
        let article_id = self.api_instance.get_article_id()?;
        let url = format!("account/articles/{}/files", article_id);
        let data = FigShareNewUpload {
            name: data_file.basename()?,
            md5: data_file.md5.clone(),
            size: data_file.size
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
        let upload_info: FigShareFile = response.json().await?;
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
                          upload_info: &FigShareFile,
                          pending_upload_info: &FigSharePendingUploadInfo,
                          path_context: &Path) -> Result<()> {
        let full_path = path_context.join(&data_file.path);
        let url = &upload_info.upload_url;
        let mut file = fs::File::open(full_path)?;

        for part in &pending_upload_info.parts {
            let start_offset = part.start_offset;
            let end_offset = part.end_offset;

            // get the binary data between these offsets
            file.seek(SeekFrom::Start(start_offset))?;
            let mut data = vec![0u8; (end_offset - start_offset + 1) as usize];
            file.read_exact(&mut data)?;

            let part_url = format!("{}/{}", &url, part.part_no);
            let _response = self.api_instance.issue_request::<HashMap<String, String>>(Method::PUT, &part_url, Some(RequestData::Binary(data)))
                .await?;
            debug!("uploaded part {} (offsets {}:{})", part.part_no, start_offset, end_offset)
        }

        Ok(())
    }

    async fn complete_upload(&self, upload_info: &FigShareFile) -> Result<()> {
        let article_id = self.api_instance.get_article_id()?;
        let url = format!("account/articles/{}/files/{}", article_id, upload_info.id);
        let data = FigShareCompleteUpload {
            id: article_id,
            name: upload_info.name.clone(),
            size: upload_info.size
        };
        self.api_instance.issue_request(Method::POST, &url, Some(RequestData::Json(data))).await?;
        Ok(())
    }

    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<()> {
        if !data_file.is_alive(path_context) {
            return Err(anyhow!("Cannot upload: file '{}' does not exist lcoally.", data_file.path));
        }
        // check if any files are associated with this article
        let article_id = self.api_instance.get_article_id()?;
        let name = data_file.basename()?;
        let existing_file = self.api_instance.file_exists(&name).await?;
        if let Some(file) = existing_file {
            if !overwrite {
                print_info!("FigShare::upload() found file '{}' in FigShare \
                            Article ID={}. Since overwrite=false, 
                            this file will not be deleted and re-upload.",
                            name, article_id);
            } else {
                info!("FigShare::upload() is deleting file '{}' since \
                      overwrite=true.", name);
                self.api_instance.delete_article_file(&file).await?;
            } 
        } 
        let (upload_info, pending_upload_info) = self.init_upload(data_file).await?;
        self.upload_parts(data_file, &upload_info, &pending_upload_info, path_context).await?;
        self.complete_upload(&upload_info).await?;
        Ok(())
    }
}

impl From<FigShareFile> for RemoteFile {
    fn from(fgsh: FigShareFile) -> Self {
        RemoteFile {
            name: fgsh.name,
            md5: Some(fgsh.computed_md5),
            size: Some(fgsh.size),
            remote_service: "FigShare".to_string(),
            url: Some(fgsh.download_url)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct FigShareArticle {
    title: String,
    id: u64
}

impl FigShareAPI {
    pub fn new(name: &str, base_url: Option<String>) -> Result<Self> {
        // Note: this constructor is not called often, except through 
        // Project::link(), since serde is usually deserializing the 
        // new FigShareAPI Remote variant from the manifest.
        let auth_keys = if base_url.is_none() {
            // using the default base_url means we're 
            // not using mock HTTP servers
            AuthKeys::new()
        } else {
            // If base_url is set, we're using mock HTTP servers,
            // so we use the test-token
            let mut auth_keys = AuthKeys::default();
            auth_keys.temporary_add("figshare", TEST_TOKEN);
            auth_keys
        };
        let token = auth_keys.get("figshare".to_string())?;
        let base_url = base_url.unwrap_or(FIGSHARE_BASE_URL.to_string());
        Ok(FigShareAPI { 
            base_url,
            article_id: None,
            name: name.to_string(), 
            token
        })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = token;
    }

    pub fn get_base_url(&self) -> String {
        self.base_url.clone()
    }

    async fn issue_request<T: serde::Serialize>(&self, method: Method, endpoint: &str,
                                                data: Option<RequestData<T>>) -> Result<Response> {
        let mut headers = HeaderMap::new();

        // FigShare will give download links outside the API, so we handle 
        // that possibility here.
        let url = if endpoint.starts_with("https://") || endpoint.starts_with("http://") {
            endpoint.to_string()
        } else {
            format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'))
        };

        trace!("request URL: {:?}", url);

        let client = Client::new();
        let mut request = client.request(method, &url);

        headers.insert("Authorization", HeaderValue::from_str(&format!("token {}", self.token)).unwrap());
        trace!("headers: {:?}", headers);
        request = request.headers(headers);

        let request = match data {
            Some(RequestData::Json(json_data)) => request.json(&json_data),
            Some(RequestData::Binary(bin_data)) => request.body(bin_data),
            Some(RequestData::File(file)) => request.body(file),
            Some(RequestData::Stream(file)) => {
                let stream = tokio_util::io::ReaderStream::new(file);
                let body = Body::wrap_stream(stream);
                request.body(body)
            },
            Some(RequestData::Empty) => request.json(&serde_json::Value::Object(serde_json::Map::new())),
            None => request,
        };

        let response = request.send().await?;
        let response_status = response.status();
        if response_status.is_success() {
            Ok(response)
        } else {
            Err(anyhow!("HTTP Error: {}\nurl: {:?}\n{:?}", response_status, url, response.text().await?))
        }
    }


    // Download a single file through the FigShare API
    async fn download_file(&self, url: &str, save_path: &Path) -> Result<()> {
        let response = reqwest::get(url).await?;
        let mut file = File::create(save_path).await?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?; // handle chunk error if needed
            file.write_all(&chunk).await?;
        }
        Ok(())
    }

    // Create a new FigShare Article
    pub async fn create_article(&self, title: &str) -> Result<FigShareArticle> {
        let endpoint = "account/articles";

        // (1) create the data for this article
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert("title".to_string(), title.to_string());
        data.insert("defined_type".to_string(), "dataset".to_string());
        debug!("creating data for article: {:?}", data);

        // (2) issue request and parse out the article ID from location
        let response = self.issue_request(Method::POST, endpoint, Some(RequestData::Json(data))).await?;
        let data = response.json::<Value>().await?;
        let article_id_result = match data.get("location").and_then(|loc| loc.as_str()) {
            Some(loc) => Ok(loc.split('/').last().unwrap_or_default().to_string()),
            None => Err(anyhow!("Response does not have 'location' set!"))
        };
        let article_id: u64 = article_id_result?.parse::<u64>().map_err(|_| anyhow!("Failed to parse article ID"))?;
        debug!("got article ID: {:?}", article_id);

        // (3) create and return the FigShareArticle
        Ok(FigShareArticle {
            title: title.to_string(),
            id: article_id,
        })
    }

    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<bool> {
        let this_upload = FigShareUpload::new(self);
        this_upload.upload(data_file, path_context, overwrite).await?;
        Ok(true)
    }

    // Get the RemoteFile.url and combine with the token to get
    // a private download link.
    //
    // Note: this is overwrite-safe: it will error out 
    // if file exists unless overwrite is true.
    //
    // Note: this cannot be moved to higher-level (e.g. Remote)
    // since each API implements authentication its own way. 
    pub fn get_download_info(&self, merged_file: &MergedFile, path_context: &Path, overwrite: bool) 
        -> Result<DownloadInfo> {
            // if local DataFile is none, not in manifest; 
            // do not download
            let data_file = match &merged_file.local {
                None => return Err(anyhow!("Cannot download() without local DataFile.")),
                Some(file) => file
            };
            // check to make sure we won't overwrite
            if data_file.is_alive(path_context) && !overwrite {
                return Err(anyhow!("Data file '{}' exists locally, and would be \
                                   overwritten by download. Use --overwrite to download.",
                                   data_file.path));
            }
            // if no remote, there is nothing to download,
            // silently return Ok. Get URL.
            let remote = merged_file.remote.as_ref().ok_or(anyhow!("Remote is None"))?;
            let url = remote.url.as_ref().ok_or(anyhow!("Cannot download; download URL not set."))?;

            // add the token in
            let url = format!("{}?token={}", url, self.token);
            let save_path = &data_file.full_path(path_context)?;
            Ok( DownloadInfo { url, path:save_path.to_string_lossy().to_string() })
        }

    // Download a single file.
    //
    // For the most part, this is deprecated, since we use the download manager 
    // "trauma" now.
    pub async fn download(&self, merged_file: &MergedFile, 
                          path_context: &Path, overwrite: bool) -> Result<()>{
        let info = self.get_download_info(merged_file, path_context, overwrite)?;
        self.download_file(&info.url, &PathBuf::from(info.path)).await?;
        Ok(())
    }

    pub async fn find_article(&self) -> Result<Option<FigShareArticle>> {
        let articles = self.get_articles().await?;
        let matches_found: Vec<_> = articles.into_iter().filter(|a| a.title == self.name).collect();
        if !matches_found.is_empty() {
            if matches_found.len() > 1 {
                return Err(anyhow!("Found multiple FigShare Articles with the \
                                   title '{}'", self.name));
            } else {
                return Ok(Some(matches_found[0].clone()));
            }
        } else {
            return Ok(None);
        }
    }

    // FigShare Remote initialization
    // 
    // This creates a FigShare article for the tracked directory.
    #[allow(unused)]
    pub async fn remote_init(&mut self, local_metadata: LocalMetadata, link_only: bool) -> Result<()> {
        // (1) Let's make sure there is no Article that exists
        // with this same name
        let found_match = self.find_article().await?;
        let article = if let Some(existing_info) = found_match {
            if !link_only {
                return Err(anyhow!("An existing FigShare Article with the title \
                                   '{}' was found. Use --link-only to link.", self.name));
            }
            existing_info
        } else {
            // Step 2: Create a new deposition if none exists
            self.create_article(&self.name).await?
        };

        // (3) Set the Article ID, which is the only state needed
        // for later queries
        self.article_id = Some(article.id);
        Ok(())
    }

    // Get FigShare Articles as FigShareArticle
    // TODO? does this get published data sets?
    async fn get_articles(&self) -> Result<Vec<FigShareArticle>> {
        let url = "/account/articles";
        let response = self.issue_request::<HashMap<String, String>>(Method::GET, url, None).await?;
        let articles: Vec<FigShareArticle> = response.json().await?;
        Ok(articles)
    }

    pub async fn get_remote_files(&self) -> Result<Vec<RemoteFile>> {
        let articles = self.get_files().await?;
        let remote_files = articles.into_iter().map(RemoteFile::from).collect();
        Ok(remote_files)
    }

    // Get all files from a FigShare Article, in a HashMap
    // with file name as keys.
    pub async fn get_files_hashmap(&self) -> Result<HashMap<String,FigShareFile>> {
        let mut files: Vec<FigShareFile> = self.get_files().await?;
        let mut files_hash: HashMap<String,FigShareFile> = HashMap::new();
        for file in files.iter_mut() {
            files_hash.insert(file.name.clone(), file.clone());
        }
        Ok(files_hash)
    }

    // Check if file exists, returning None if not,
    // and the FigShareFile if so
    pub async fn file_exists(&self, name: &str) -> Result<Option<FigShareFile>> {
        let files = self.get_files_hashmap().await?;
        Ok(files.get(name).cloned())
    }

    pub fn get_article_id(&self) -> Result<u64> {
        let article_id  = self.article_id.ok_or(anyhow!("Internal Error: FigShare.article_id is None."))?;
        Ok(article_id)
    }

    // Get all files from the FigShare Article
    pub async fn get_files(&self) -> Result<Vec<FigShareFile>> {
        let article_id = self.get_article_id()?;
        let url = format!("/account/articles/{}/files", article_id);
        let response = self.issue_request::<HashMap<String,String>>(Method::GET, &url, None).await?;
        let files: Vec<FigShareFile> = response.json().await?;
        Ok(files)
    }

    // Delete Article
    /* async fn delete_article(&self, article: &FigShareArticle) -> Result<()> {
       let url = format!("account/articles/{}", article.id);
       self.issue_request::<HashMap<String, String>>(Method::DELETE, &url, None).await?;
       Ok(())
       }
       */

    // Delete the specified file from the FigShare Article
    // 
    // Note: we require a &FigShareFile as a way to enforce it exists,
    // e.g. is the result of a previous query.
    async fn delete_article_file(&self, file: &FigShareFile) -> Result<()> {
        let article_id = self.get_article_id()?;
        let url = format!("account/articles/{}/files/{}", article_id, file.id);
        self.issue_request::<HashMap<String,String>>(Method::DELETE, &url, None).await?;
        info!("deleted FigShare file '{}' (Article ID={})", file.name, article_id);
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use crate::logging_setup::setup;


    #[tokio::test]
    async fn test_create_article() {
        setup();
        // Start a mock server
        let server = MockServer::start();

        let expected_id = 12345;
        let title = "Test Article";

        // Create a mock endpoint for creating an article
        let create_article_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/account/articles")
                .header("Authorization", &format!("token {}", TEST_TOKEN.to_string()))
                .json_body(json!({
                    "title": title.to_string(),
                    "defined_type": "dataset"
                }));
            then.status(201)
                .json_body(json!({
                    "location": format!("{}account/articles/{}", server.url(""), expected_id)
                }));
        });

        // Define a sample title for the article
        let api = FigShareAPI::new("Test Article", Some(server.url(""))).unwrap();

        info!("auth_keys: {:?}", api.token);
        // Call the create_article method
        let result = api.create_article(title).await;

        // Check the result
        assert_eq!(result.is_ok(), true);
        let article = result.unwrap();
        assert_eq!(article.title, title);
        assert_eq!(article.id, expected_id);

        // Verify that the mock was called exactly once
        create_article_mock.assert();
    } 

}
