use std::fs;
use anyhow::{anyhow,Result};
use std::path::Path;
use reqwest::{Method, header::{HeaderMap, HeaderValue, CONTENT_TYPE}};
use reqwest::{Client, Response};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
use serde_json::Value;
#[allow(unused_imports)]
use log::{info, trace, debug};

use crate::data::{DataFile, MergedFile};
use crate::remote::{AuthKeys,RemoteFile,DownloadInfo,RequestData};


const BASE_URL: &str = "https://zenodo.org/api/deposit/depositions";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoDeposition {
    conceptrecid: String,
    created: String,
    files: Vec<String>,
    id: u32,
    links: ZenodoLinks,
    metadata: Metadata,
    modified: String,
    owner: u32,
    record_id: u32,
    state: String,
    submitted: bool,
    title: String,
}


#[derive(Debug, Deserialize)]
struct ZenodoFile {
    key: String,
    mimetype: String,
    checksum: String,
    version_id: String,
    size: usize,
    created: String,
    updated: String,
    links: HashMap<String, String>,
    is_head: bool,
    delete_marker: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoLinks {
    bucket: String,
    discard: String,
    edit: String,
    files: String,
    html: String,
    latest_draft: String,
    latest_draft_html: String,
    publish: String,
    #[serde(rename = "self")]
    self_link: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Metadata {
    prereserve_doi: PrereserveDoi,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PrereserveDoi {
    doi: String,
    recid: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoAPI {
    name: String,
    #[serde(skip_serializing, skip_deserializing)]
    token: String,
    deposition: Option<ZenodoDeposition>
}

impl ZenodoAPI {
    pub fn new(name: String) -> Result<Self> {
        let auth_keys = AuthKeys::new();
        let token = auth_keys.get("figshare".to_string())?;
        Ok(ZenodoAPI { 
            name, 
            token,
            deposition: None
        })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = token;
    }

    // issue request
    // TODO: this is the same as FigShareAPI's issue_request().
    // Since APIs can have different authentication routines, we
    // should handle that part separately.
    async fn issue_request<T: serde::Serialize>(&self, method: Method, url: &str,
                                                headers: Option<HeaderMap>,
                                                data: Option<RequestData<T>>) -> Result<Response> {
        assert!(url.starts_with("https://"));
        let url = format!("{}?access_token={}", url, self.token);
        trace!("request URL: {:?}", &url);

        let client = Client::new();
        let mut request = client.request(method, &url);
        if let Some(h) = headers {
            info!("Request Headers: {:?}", h);
            request = request.headers(h);
        }

        let request = match data {
            Some(RequestData::Json(json_data)) => request.json(&json_data),
            Some(RequestData::Binary(bin_data)) => request.body(bin_data),
            Some(RequestData::File(file)) => request.body(file),
            Some(RequestData::Empty) => request.json(&serde_json::Value::Object(serde_json::Map::new())),
            None => request,
        };

        let response = request.send().await?;

        let response_status = response.status();
        if response_status.is_success() {
            Ok(response)
        } else {
            Err(anyhow!("HTTP Error: {}\nurl: {:?}\n{:?}", response_status, &url, response.text().await?))
        }
    }

    pub async fn remote_init(&mut self) -> Result<()> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let response = self.issue_request::<HashMap<String, String>>(Method::POST, BASE_URL, Some(headers), Some(RequestData::Empty)).await?;
        println!("{:?}", response);
        let info: ZenodoDeposition = response.json().await?;
        self.deposition = Some(info);
        Ok(())
    }

    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<()> {
        let bucket_url = self.bucket_url;
        let full_path = path_context.join(&data_file.path);
        let file = tokio::fs::File::open(full_path).await?;
        let response = self.issue_request::<HashMap<String, String>>(Method::PUT, &bucket_url, None, Some(RequestData::File(file))).await?;
        let info: ZenodoFile = response.json().await?;
        Ok(())
    }

    pub async fn get_files(&self) -> Result<()> {
        let response = self.issue_request::<HashMap<String, String>>(Method::GET, BASE_URL, None, None).await?;
        println!("{:?}", response);
        //let upload_info: ZenodoDeposition = response.json().await?;
        Ok(())
    }

    pub async fn get_remote_files(&self) -> Result<Vec<RemoteFile>> {
        let articles = self.get_files().await?;
        //let remote_files = articles.into_iter().map(RemoteFile::from).collect();
        let remote_files: Vec<RemoteFile> = Vec::new();
        Ok(remote_files)
    }
}
