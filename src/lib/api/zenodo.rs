use anyhow::{anyhow,Result,Context};
use std::path::Path;
use reqwest::{Method, header::{HeaderMap, HeaderValue, CONTENT_TYPE}};
use reqwest::{Client, Response};
use std::collections::HashMap;
use serde_derive::{Serialize,Deserialize};
#[allow(unused_imports)]
use log::{info, trace, debug};
use colored::Colorize;
use std::convert::TryInto;

#[allow(unused_imports)]
use crate::{print_info,print_warn};


use crate::lib::{data::{DataFile, MergedFile}, project::LocalMetadata, remote::DownloadInfo};
use crate::lib::remote::{AuthKeys,RemoteFile,RequestData};


const BASE_URL: &str = "https://zenodo.org/api";

// for testing:
const TEST_TOKEN: &str = "test-token";

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


#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
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
        let description = self.description.unwrap_or("Upload by SciDataFlow.".to_string());

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

// Remove the BASE_URL from full URLs, e.g. for 
// bucket_urls provided by Zenodo so they can go through the common 
// issue_request() method
fn remove_base_url(full_url: &str) -> Result<String> {
    full_url.strip_prefix(BASE_URL).map(|s| s.to_string())
        .ok_or(anyhow!("Internal error: Zenodo BASE_URL not found in full URL: full_url={:?}, BASE_URL={:?}",
                       full_url, BASE_URL))
}

// for serde deserialize default
fn zenodo_api_url() -> String {
    BASE_URL.to_string()
}


#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ZenodoAPI {
    #[serde(skip_serializing, skip_deserializing,default="zenodo_api_url")]
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
    pub fn new(name: &str, base_url: Option<String>) -> Result<Self> {
        // Note: this constructor is not called often, except through 
        // Project::link(), since serde is usually deserializing the 
        // new ZenodoAPI Remote variant from the manifest.
        let auth_keys = if base_url.is_none() {
            // using the default base_url means we're 
            // not using mock HTTP servers
            AuthKeys::new()
        } else {
            // If base_url is set, we're using mock HTTP servers,
            // so we use the test-token
            let mut auth_keys = AuthKeys::default();
            auth_keys.temporary_add("zenodo", TEST_TOKEN);
            auth_keys
        };
        let token = auth_keys.get("zenodo".to_string())?;
        let base_url = base_url.unwrap_or(BASE_URL.to_string());
        Ok(ZenodoAPI { 
            base_url,
            name: name.to_string(), 
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
        trace!("request: {:?}", request);
        if let Some(h) = headers {
            trace!("Request Headers: {:?}", h);
            request = request.headers(h);
        }

        if let Some(data) = &data { // Use the cloned data for logging
            let data_clone = data.clone(); // Clone the data
            trace!("Request Data: {:?}", data_clone);
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
        trace!("ZenodoDeposition: {:?}", info);
        self.deposition_id = Some(info.id as u64);
        self.bucket_url = info.links.bucket;
        Ok(())
    }

    // Check if file exists, returning None if not,
    // and the ZenodoFile if so
    // TODO: could be part of higher Remote API, e.g. through generics?
    pub async fn file_exists(&self, name: &str) -> Result<Option<ZenodoFile>> {
        let files = self.get_files_hashmap().await?;
        Ok(files.get(name).cloned())
    }

    pub fn get_deposition_id(&self) -> Result<u64> {
        self.deposition_id.ok_or(anyhow!("Internal Error: Zenodo deposition_id not set."))
    }

    pub async fn delete_article_file(&self, file: &ZenodoFile) -> Result<()> {
        let id = self.get_deposition_id()?;
        let file_id = &file.id;
        let url = format!("{}/{}/files/{}", "/deposit/depositions", id, file_id);
        self.issue_request::<HashMap<String,String>>(Method::DELETE, &url, None, None).await?;
        info!("deleted Zenodo file '{}' (File ID={})", file.filename, file_id);
        Ok(())
    }


    // Upload the file, deleting any existing files if overwrite is true.
    //
    // Returns true/false if upload was completed or not. Will Error in other cases.
    #[allow(unused_variables)]
    pub async fn upload(&self, data_file: &DataFile, path_context: &Path, overwrite: bool) -> Result<bool> {
        let bucket_url = self.bucket_url.as_ref().ok_or(anyhow!("Internal Error: Zenodo bucket_url not set. Please report."))?;
        let full_path = path_context.join(&data_file.path);

        let name = data_file.basename()?;
        let existing_file = self.file_exists(&name).await?;
        let id = self.get_deposition_id()?;

        let bucket_endpoint = remove_base_url(bucket_url)?;
        let bucket_endpoint = format!("{}/{}", bucket_endpoint, name);

        // handle deleting files first if a file exists and overwrite is true
        if let Some(file) = existing_file {
            if !overwrite {
                print_info!("Zenodo::upload() found file '{}' in Zenodo \
                            Deposition ID={}. Since overwrite=false, 
                            this file will not be deleted and re-uploaded.",
                            name, id);
                return Ok(false);
            } else {
                info!("FigShare::upload() is deleting file '{}' since \
                      overwrite=true.", name);
                self.delete_article_file(&file).await?;
            } 
        } 

        // upload the file
        let file = tokio::fs::File::open(full_path).await?;
        let response = self.issue_request::<HashMap<String, String>>(Method::PUT, &bucket_endpoint, None, Some(RequestData::File(file))).await?;
        let info: ZenodoFileUpload = response.json().await?;

        let msg = "After upload, the local and remote MD5s differed. SciDataFlow\n\
                    automatically deletes the remote file in this case";
        // let's compare the MD5s
        let remote_md5 = info.checksum;
        if remote_md5 != data_file.md5 {
            let zenodo_file = self.file_exists(&info.key).await?;
            match zenodo_file {
                None => {
                    // The MD5s disagree, but when we try to get the file, we also cannot
                    // find it. This is an extreme corner case, likely due to issues on 
                    // Zenodo's end
                    Err(anyhow!("{}; however,\n\
                                in trying this, the remote file could not be found. This \n\
                                very likely reflects an internal error on Zenodo's end.\n\
                                Please log into Zenodo.org and manaually delete the file \n\
                                (if it exists) and try re-uploading.", msg))
                },
                Some(file) => {
                    self.delete_article_file(&file).await
                        .context(format!("{}. However, scidataflow encountered an error while \
                                         trying to delete the file.", msg))?;
                    Err(anyhow!("{}.\n\
                                Please try re-uploading. If this problem persists, please\n\
                                report it to Zenodo and file an issue at:\n\
                                https://github.com/vsbuffalo/scidataflow/issues", msg))
                }
            }
        } else {
            // we did the upload, MD5s match
            Ok(true)
        }
    }

    pub async fn get_files(&self) -> Result<Vec<ZenodoFile>> {
        let id = self.get_deposition_id()?;
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

    // Get all files from a Zenodo Deposition, in a HashMap
    // with file name as keys.
    pub async fn get_files_hashmap(&self) -> Result<HashMap<String,ZenodoFile>> {
        let mut files: Vec<ZenodoFile> = self.get_files().await?;
        let mut files_hash: HashMap<String,ZenodoFile> = HashMap::new();
        for file in files.iter_mut() {
            files_hash.insert(file.filename.clone(), file.clone());
        }
        Ok(files_hash)
    }


    // Get the RemoteFile.url and combine with the token to get
    // a private download link.
    //
    // Note: this is overwrite-safe: it will error out 
    // if file exists unless overwrite is true.
    //
    // Note: this *cannot* be moved to higher-level (e.g. Remote)
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
            let url = format!("{}?access_token={}", url, self.token);
            let save_path = &data_file.full_path(path_context)?;
            Ok( DownloadInfo { url, path:save_path.to_string_lossy().to_string() })
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use crate::logging_setup::setup;
    use std::io::Write;

    #[tokio::test]
    async fn test_remote_init_success() {
        setup();
        // Start a mock server
        let server = MockServer::start();

        let expected_id = 12345;
        let expected_bucket_url = "http://zenodo.com/api/some-link-to-bucket";

        // Prepare local_metadata
        let local_metadata = LocalMetadata {
            author_name: Some("Joan B. Scientist".to_string()),
            title: Some("A *truly* reproducible project.".to_string()),
            email: None,
            affiliation: Some("UC Berkeley".to_string()),
            description: Some("Let's build infrastructure so science can build off itself.".to_string()),
        };

        // Create a mock deposition endpoint with a simulated success response
        let deposition_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/deposit/depositions");
            // TODO probably could minimize this example
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
                            "affiliation": local_metadata.affiliation,
                            "name": local_metadata.author_name,
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
        let mut api = ZenodoAPI::new("test", Some(server.url("/"))).unwrap();

        // Main call to test
        let _result = api.remote_init(local_metadata).await;
        //info!("result: {:?}", result);

        // ensure the specified mock was called exactly one time (or fail).
        deposition_mock.assert();

        // Assert that the deposition_id and bucket_url have been set correctly
        assert_eq!(api.deposition_id, Some(expected_id as u64));
        assert_eq!(api.bucket_url, Some(expected_bucket_url.to_string()));
    }

    #[tokio::test]
    async fn test_delete_article_file() {
        setup();
        // Start a mock server
        let server = MockServer::start();

        let file = ZenodoFile {
            checksum: "fake-checksum".to_string(),
            filename: "fake_data.tsv".to_string(),
            id: "56789".to_string(),
            links: ZenodoLinks::default(),
            filesize: 11
        };

        let expected_deposition_id = 1234564;

        // Mock for delete_article_file
        let delete_file_mock = server.mock(|when, then| {
            when.method(DELETE)
                .path(format!("/deposit/depositions/{}/files/{}", 
                              expected_deposition_id, file.id))
                .query_param("access_token", TEST_TOKEN);
            then.status(200); // Assuming a successful deletion returns a 200 status code
        });

        // Create an instance of your API class and set the deposition_id
        let mut api = ZenodoAPI::new("test", Some(server.url("/"))).unwrap();
        trace!("auth_keys: {:?}", api.token);
        api.deposition_id = Some(expected_deposition_id);

        // Main call to test
        let result = api.delete_article_file(&file).await;

        // Assert that the result is OK
        assert!(result.is_ok(), "Err encountered in Zenodo::delete_article_file(): {:?}", result);

        // Ensure the specified mock was called exactly once
        delete_file_mock.assert();
    }

    async fn test_upload(file_exists: bool, overwrite: bool) -> Result<bool> {
        setup();
        // Start a mock server
        let server = MockServer::start();

        // Use the tempfile crate to create a temporary file
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        // Write some content to the temporary file if necessary
        writeln!(temp_file, "Some test data for the file").unwrap();
        // Get the path to the temporary file
        let temp_file_path = temp_file.path().to_owned();

        // (note: MD5s are fake, no checking with the mock server)
        let temp_filename = temp_file_path.to_string_lossy().to_string();
        let md5 = "2942bfabb3d05332b66eb128e0842cff";
        let size = 1024;
        let data_file = DataFile {
            path: temp_filename.clone(),
            tracked: true,
            md5: md5.to_string(),
            size,
        };

        let path_context = Path::new("path/to/datafile");
        let expected_deposition_id = 1234564;
        let bucket_endpoint = "/files/568377dd-daf8-4235-85e1-a56011ad454b";
        let bucket_url = format!("{}/{}", BASE_URL, bucket_endpoint);

        // Mock for the get_files method
        let mut remote_files = Vec::new();
        let zenodo_file = ZenodoFile {
            checksum: md5.clone().to_string(),
            filename: data_file.basename()?,
            filesize: size as usize,
            id: "4242".to_string(),
            links: ZenodoLinks::default()
        };
        if file_exists {
            remote_files.push(zenodo_file.clone());
        }

        let get_files_mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/deposit/depositions/{}/files", expected_deposition_id))
                .query_param("access_token", TEST_TOKEN);
            then.status(200)
                // return the files found, which depends on params of test
                .json_body(json!(remote_files));
        });

        // Mock for the delete_article_file method
        let delete_file_mock = if file_exists && overwrite {
            info!("delete_file_mock had been created");
            Some(server.mock(|when, then| {
                let expected_file_id = &zenodo_file.id;
                when.method(DELETE)
                    .path(format!("/deposit/depositions/{}/files/{}", expected_deposition_id, expected_file_id))
                    .query_param("access_token", TEST_TOKEN);
                then.status(204); // Typically, HTTP status 204 indicates that the server successfully processed the request and is not returning any content.
            }))
        } else {
            None
        };

        // Mock for the upload method
        let upload_file_mock = server.mock(|when, then| {
            when.method(PUT)
                .path_matches(Regex::new(&format!(r"{}/([^/]+)", bucket_endpoint)).unwrap());
            then.status(201)
                .json_body(json!({
                    "key": "example_data_file.tsv",
                    "mimetype": "application/zip",
                    "checksum": md5,
                    "version_id": "38a724d3-40f1-4b27-b236-ed2e43200f85",
                    "size": size,
                    "created": "2020-02-26T14:20:53.805734+00:00",
                    "updated": "2020-02-26T14:20:53.811817+00:00",
                    "links": {
                        "self": "https://zenodo.org/api/files/44cc40bc-50fd-4107-b347-00838c79f4c1/dummy_example.pdf",
                        "version": "https://zenodo.org/api/files/44cc40bc-50fd-4107-b347-00838c79f4c1/dummy_example.pdf?versionId=38a724d3-40f1-4b27-b236-ed2e43200f85",
                        "uploads": "https://zenodo.org/api/files/44cc40bc-50fd-4107-b347-00838c79f4c1/dummy_example.pdf?uploads"
                    },
                    "is_head": true,
                    "delete_marker": false
                }));
        });

        // Create an instance of your API class and set the deposition_id
        let mut api = ZenodoAPI::new("test", Some(server.url("/"))).unwrap();
        api.deposition_id = Some(expected_deposition_id);
        api.bucket_url = Some(bucket_url.to_string());

        // Main call to test
        let result = api.upload(&data_file, &path_context, overwrite).await;

        // Ensure the specified mocks were called exactly one time (or fail).
        get_files_mock.assert();
        if !file_exists {
            upload_file_mock.assert();
        }
        if file_exists && overwrite {
            upload_file_mock.assert();
            delete_file_mock.unwrap().assert();
        }
        return result
    }

    #[tokio::test]
    async fn test_upload_no_overwrite_no_remote_files() -> Result<()> {
        let result = test_upload(false, false).await?;
        assert!(result, "Zenodo::upload() failed (file_exists={:?}, overwrite={:?}). Result: {:?}", 
                false, false, result);
        Ok(())
    }

    #[tokio::test]
    async fn test_upload_no_overwrite_with_remote_files() -> Result<()> {
        let result = test_upload(true, false).await?;
        assert!(!result, "Zenodo::upload() failed (file_exists={:?}, overwrite={:?}). Result: {:?}", 
                true, false, result);
        Ok(())
    }

    #[tokio::test]
    async fn test_upload_overwrite_with_remote_files() -> Result<()> {
        let result = test_upload(true, true).await?;
        assert!(result, "Zenodo::upload() failed (file_exists={:?}, overwrite={:?}). Result: {:?}", 
                true, true, result);
        Ok(())
    }
}

