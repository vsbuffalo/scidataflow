use std::path::{PathBuf};
use super::data::StatusEntry;

/// A Status Summary, for tracked, untracked, remote-only, etc. files 
pub trait Status {
    fn status(&self, path_context: &PathBuf, abbrev: Option<i32>) -> StatusEntry;
}
