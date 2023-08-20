use anyhow::{anyhow,Result};
use std::path::Path;
use reqwest::{Method, header::{HeaderMap, HeaderValue, CONTENT_TYPE}};
use reqwest::{Client, Response};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
#[allow(unused_imports)]
use log::{info, trace, debug};
use std::convert::TryInto;

use crate::lib::{data::DataFile, project::LocalMetadata};
use crate::lib::remote::{AuthKeys,RemoteFile,RequestData};


const BASE_URL: &str = "https://zenodo.org/api";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoDeposition {
    conceptrecid: String,
    created: String,
    #[serde(skip_deserializing)]
    files: Vec<String>,
    id: u32,
    links: ZenodoLinks,
    metadata: ZenodoMetadata,
    modified: String,
    owner: u32,
    record_id: u32,
    state: String,
    submitted: bool,
    title: String,
}


#[allow(dead_code)]  // used for deserialization of requests
#[derive(Debug, Deserialize)]
pub struct ZenodoFileUpload {
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


#[derive(Debug, Serialize, Deserialize)]
pub struct ZenodoFile {
    checksum: String,
    filename: String,
    filesize: usize,
    id: String,
    links: ZenodoLinks,
}

impl From<ZenodoFile> for RemoteFile {
    fn from(znd: ZenodoFile) -> Self {
        RemoteFile {
            name: znd.filename,
            md5: Some(znd.checksum),
            size: Some(znd.filesize as u64),
            remote_service: "Zenodo".to_string(),
            url: znd.links.download
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoLinks {
    download: Option<String>,
    bucket: Option<String>,
    discard: Option<String>,
    edit: Option<String>,
    files: Option<String>,
    html: Option<String>,
    latest_draft: Option<String>,
    latest_draft_html: Option<String>,
    publish: Option<String>,
    #[serde(rename = "self")]
    self_link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Creator {
    name: String,
    affiliation: Option<String>
}

// We need this wrapper to provide the metadata
// for the Zenodo Deposition.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ZenodoDepositionData {
    metadata: ZenodoMetadata,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct ZenodoMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    prereserve_doi: Option<PrereserveDoi>,
    title: String,
    upload_type: Option<String>,
    description: Option<String>,
    creators: Option<Vec<Creator>>,
}

impl TryInto<ZenodoDepositionData> for LocalMetadata {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<ZenodoDepositionData> {
        let name = self.author_name.ok_or_else(|| anyhow!("Author name is required"))?;
        // TODO? Warn user of default description?
        let description = self.description.unwrap_or("Upload by SciFlow.".to_string());

        Ok(ZenodoDepositionData {
            metadata: ZenodoMetadata {
                prereserve_doi: None,
                title: self.title.ok_or(anyhow!("Zenodo requires a title be set."))?,
                upload_type: Some("dataset".to_string()),
                description: Some(description),
                creators: Some(vec![Creator {
                    name,
                    affiliation: self.affiliation,
                }]),
            },
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PrereserveDoi {
    doi: String,
    recid: usize,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoAPI {
    #[serde(skip_serializing, skip_deserializing)]
    base_url: String,
    name: String,
    #[serde(skip_serializing, skip_deserializing)]
    token: String,
    // Minimal info for other API operations:
    // Note: could store the whole ZenodoDeposition but
    // this is rather lengthy.
    deposition_id: Option<u64>,
    bucket_url: Option<String>,
}

impl ZenodoAPI {
    pub fn new(name: String, base_url: Option<String>) -> Result<Self> {
        let auth_keys = AuthKeys::new();
        let token = auth_keys.get("figshare".to_string())?;
        let base_url = base_url.unwrap_or(BASE_URL.to_string());
        Ok(ZenodoAPI { 
            base_url,
            name, 
            token,
            deposition_id: None,
            bucket_url: None
        })
    }

    pub fn set_token(&mut self, token: String) {
        self.token = token;
    }

    // issue request
    // TODO: this is the same as FigShareAPI's issue_request().
    // Since APIs can have different authentication routines, we
    // should handle that part separately.
    async fn issue_request<T: serde::Serialize + std::fmt::Debug>(&self, method: Method, endpoint: &str,
                                                                  headers: Option<HeaderMap>,
                                                                  data: Option<RequestData<T>>) -> Result<Response> {
        let url = format!("{}/{}?access_token={}", self.base_url.trim_end_matches('/'), endpoint.trim_start_matches('/'), self.token);
        trace!("request URL: {:?}", &url);

        let client = Client::new();
        let mut request = client.request(method, &url);
        info!("request: {:?}", request);
        if let Some(h) = headers {
            info!("Request Headers: {:?}", h);
            request = request.headers(h);
        }

        if let Some(data) = &data { // Use the cloned data for logging
            let data_clone = data.clone(); // Clone the data
            info!("Request Data: {:?}", data_clone);
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


    // Initialize the data collection on the Remote
    //
    // For Zenodo, this creates a new "deposition"
    #[allow(unused)]
    pub async fn remote_init(&mut self, local_metadata: LocalMetadata) -> Result<()> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let metadata: ZenodoDepositionData = local_metadata.try_into()?;
        let data = Some(RequestData::Json(metadata));
        let response = self.issue_request(Method::POST, "/deposit/depositions", Some(headers), data).await?;
        let info: ZenodoDeposition = response.json().await?;
        info!("ZenodoDeposition: {:?}", info);
        self.deposition_id = Some(info.id as u64);
        self.bucket_url = info.links.bucket;
        Ok(())
    }
    
    #[allow(unused_variables)]
    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<()> {
        // TODO implement overwrite
        let bucket_url = self.bucket_url.as_ref().ok_or(anyhow!("Internal Error: Zenodo bucket_url not set."))?;
        let full_path = path_context.join(&data_file.path);
        let file = tokio::fs::File::open(full_path).await?;
        let response = self.issue_request::<HashMap<String, String>>(Method::PUT, bucket_url, None, Some(RequestData::File(file))).await?;
        let _info: ZenodoFileUpload = response.json().await?;
        Ok(())
    }

    pub async fn get_files(&self) -> Result<Vec<ZenodoFile>> {
        let id = self.deposition_id.ok_or(anyhow!("Internal Error: Zenodo deposition_id not set."))?;
        let url = format!("{}/{}/files", "/deposit/depositions", id);
        let response = self.issue_request::<HashMap<String, String>>(Method::GET, &url, None, None).await?;
        let files: Vec<ZenodoFile> = response.json().await?;
        Ok(files)
    }

    pub async fn get_remote_files(&self) -> Result<Vec<RemoteFile>> {
        let articles = self.get_files().await?;
        let remote_files:Vec<RemoteFile> = articles.into_iter().map(RemoteFile::from).collect();
        Ok(remote_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use lazy_static::lazy_static;
    use std::sync::Once;

    fn setup() {
        lazy_static! {
            static ref INIT_LOGGING: Once = Once::new();
        }

        INIT_LOGGING.call_once(|| {
            env_logger::init();
        });
    }

    #[tokio::test]
    async fn test_remote_init_success() {
        setup();
        // Start a mock server
        let server = MockServer::start();

        let expected_id = 12345;
        let expected_bucket_url = "http://zenodo.com/api/some-link-to-bucket";

        // Create a mock deposition endpoint with a simulated success response
        let deposition_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/deposit/depositions");
            then.status(200)
                .json_body(json!({
                    "conceptrecid": "8266447",
                    "created": "2023-08-20T01:31:12.406094+00:00",
                    "doi": "",
                    "doi_url": "https://doi.org/",
                    "files": [],
                    "id": expected_id,
                    "links": {
                        "bucket": expected_bucket_url,
                        "discard": "https://zenodo.org/api/deposit/depositions/8266448/actions/discard",
                        "edit": "https://zenodo.org/api/deposit/depositions/8266448/actions/edit",
                        "files": "https://zenodo.org/api/deposit/depositions/8266448/files",
                        "html": "https://zenodo.org/deposit/8266448",
                        "latest_draft": "https://zenodo.org/api/deposit/depositions/8266448",
                        "latest_draft_html": "https://zenodo.org/deposit/8266448",
                        "publish": "https://zenodo.org/api/deposit/depositions/8266448/actions/publish",
                        "self": "https://zenodo.org/api/deposit/depositions/8266448"
                    },
                    "metadata": {
                        "access_right": "open",
                        "creators": [
                        {
                            "affiliation": "Zenodo",
                            "name": "Doe, John"
                        }
                        ],
                        "description": "This is a description of my deposition",
                        "doi": "",
                        "license": "CC-BY-4.0",
                        "prereserve_doi": {
                            "doi": "10.5281/zenodo.8266448",
                            "recid": 8266448
                        },
                        "publication_date": "2023-08-20",
                        "title": "My Deposition Title",
                        "upload_type": "poster"
                    },
                    "modified": "2023-08-20T01:31:12.406103+00:00",
                    "owner": 110965,
                    "record_id": 8266448,
                    "state": "unsubmitted",
                    "submitted": false,
                    "title": "My Deposition Title"
                }));
        });

        // Create an instance of ZenodoAPI
        let mut api = ZenodoAPI::new("test".to_string(), Some(server.url("/"))).unwrap();
        info!("Test ZenodoAPI: {:?}", api);
        api.set_token("fake_token".to_string());

        // Prepare local_metadata
        let local_metadata = LocalMetadata {
            author_name: Some("Joan B. Scientist".to_string()),
            title: Some("A *truly* reproducible project.".to_string()),
            email: None,
            affiliation: None,
            description: Some("Let's build infrastructure so science can build off itself.".to_string()),
        };

        // main call to test
        let result = api.remote_init(local_metadata).await;
        info!("result: {:?}", result);

        // ensure the specified mock was called exactly one time (or fail).
        deposition_mock.assert();

        // Assert that the deposition_id and bucket_url have been set correctly
        assert_eq!(api.deposition_id, Some(expected_id as u64));
        assert_eq!(api.bucket_url, Some(expected_bucket_url.to_string()));
    }
}

