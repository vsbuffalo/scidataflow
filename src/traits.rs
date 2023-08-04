use std::path::{PathBuf};
use super::data::StatusEntry;

/// Status trait for tracked files, remote items, etc.
pub trait Status {
    fn status(&self, path_context: &PathBuf, abbrev: Option<i32>) -> StatusEntry;
}
