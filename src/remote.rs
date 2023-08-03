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
/*
pub struct RequestHandler {
    client: reqwest::Client,
    auth_key: AuthKey,
}

impl RequestHandler {
    pub fn new(auth_key_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let auth_key = fs::read_to_string(auth_key_path)?;

        Ok(Self { client, auth_key })
    }

    pub async fn get(&self, url: &str) -> Result<String, Box<dyn Error>> {
        let res = self.client.get(url).send().await?;
        let body = res.text().await?;
        Ok(body)
    }

    pub async fn post(&self, url: &str, body: &str) -> Result<String, Box<dyn Error>> {
        let res = self.client.post(url).body(String::from(body)).send().await?;
        let body = res.text().await?;
        Ok(body)
    }

}

pub struct FigShareAPI {
    request_handler: RequestHandler,
}

impl FigShareAPI {
    pub fn new(auth_key: &String) -> Self {
        Self {
            request_handler: RequestHandler::new(),
        }
    }
}

impl RemoteAPI for FigShareAPI {
    fn upload() -> Result<(), Box<dyn std::error::Error>> {
        // implement upload for figshare
    }

    fn download() -> Result<(), Box<dyn std::error::Error>> {
        // implement download for figshare
    }
} */

