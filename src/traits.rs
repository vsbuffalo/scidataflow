use std::path::{PathBuf};
use anyhow::Result;
use super::data::StatusEntry;

/// Status trait for tracked files, remote items, etc.
pub trait Status {
    fn status_info(&self, path_context: &PathBuf, abbrev: Option<i32>) -> Result<StatusEntry>;
}
