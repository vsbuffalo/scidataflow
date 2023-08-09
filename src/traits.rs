use std::path::{PathBuf};
use anyhow::Result;
use std::collections::HashMap;


use crate::data::StatusEntry;
use crate::remote::Remote;

/// Status trait for tracked files, remote items, etc.
pub trait Status {
    fn status_info(&self, path_context: &PathBuf, remotes: HashMap<String,Remote>, n: Option<i32>) -> Result<StatusEntry>;
}
