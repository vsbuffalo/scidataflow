use std::path::{PathBuf};
use super::data::StatusEntry;

/// Status trait for tracked files, remote items, etc.
pub trait Status {
    fn status(&self, path_context: &PathBuf, abbrev: Option<i32>) -> StatusEntry;
}

/// Traits for interacting with Remote APIs
pub trait RemoteAPI {
    fn upload(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn download(&self, file_id: &str, destination_path: &str) -> Result<(), Box<dyn std::error::Error>>;
}
