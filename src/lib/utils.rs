use anyhow::{anyhow, Result};
use chrono::{Local, Utc};
use colored::*;
#[allow(unused_imports)]
use log::{debug, info, trace};
use md5::Context;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::ops::Add;
use std::path::{Path, PathBuf};
use timeago::Formatter;

use crate::lib::data::StatusEntry;
use crate::lib::remote::Remote;

use super::data::LocalStatusCode;
use super::remote::RemoteStatusCode;

pub const ISSUE_URL: &str = "https://github.com/vsbuffalo/scidataflow/issues";

pub fn load_file(path: &PathBuf) -> String {
    let mut file = File::open(path).expect("unable to open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("unable to read file");
    contents
}

pub fn ensure_directory(dir: &Path) -> Result<()> {
    let path = Path::new(dir);
    if path.is_dir() {
        Ok(())
    } else {
        Err(anyhow!(
            "'{}' is not a directory or doesn't exist.",
            dir.to_string_lossy()
        ))
    }
}

pub fn is_directory(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

pub fn ensure_exists(path: &Path) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(anyhow!("Path does not exist: {:?}", path))
    }
}

/// Compute the MD5 of a file returning None if the file is empty.
pub async fn compute_md5(file_path: &Path) -> Result<Option<String>> {
    const BUFFER_SIZE: usize = 1024;

    let mut file = match File::open(file_path) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };

    let mut buffer = [0; BUFFER_SIZE];
    let mut md5 = Context::new();

    loop {
        let bytes_read = match file.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(bytes_read) => bytes_read,
            Err(e) => return Err(anyhow!("I/O reading file: {:?}", e)),
        };

        md5.consume(&buffer[..bytes_read]);
    }

    let result = md5.compute();
    Ok(Some(format!("{:x}", result)))
}

/// Get the directory at the specified depth from a path string
fn get_dir_at_depth(dir: &str, filename: &str, depth: usize) -> String {
    // Combine directory and filename into a full path
    let full_path = if dir.is_empty() {
        Path::new(filename).to_path_buf()
    } else {
        Path::new(dir).join(filename).to_path_buf()
    };

    // Get the parent directory of the full path
    let parent_path = full_path.parent().unwrap_or(Path::new("."));

    // Split the parent path into components
    let components: Vec<_> = parent_path.components().collect();

    if depth == 0 || components.is_empty() {
        return ".".to_string();
    }

    // Take components up to the specified depth
    let depth_path: PathBuf = components
        .iter()
        .take(depth.min(components.len()))
        .collect();

    if depth_path.as_os_str().is_empty() {
        ".".to_string()
    } else {
        depth_path.to_string_lossy().to_string()
    }
}

pub fn print_fixed_width_status_short(
    rows: BTreeMap<DirectoryEntry, Vec<StatusEntry>>,
    color: bool,
    all: bool,
    short: bool,
    depth: Option<usize>,
    has_remote_info: bool,
) {
    // If depth is provided, reorganize the data based on the specified depth
    let grouped_rows: BTreeMap<DirectoryEntry, Vec<StatusEntry>> = if let Some(depth) = depth {
        let mut depth_grouped: BTreeMap<DirectoryEntry, Vec<StatusEntry>> = BTreeMap::new();
        for (dir_entry, entries) in rows {
            for entry in entries {
                let base_dir = get_dir_at_depth(&dir_entry.path, &entry.name, depth);
                depth_grouped
                    .entry(DirectoryEntry {
                        path: base_dir,
                        remote_name: None,
                    })
                    .or_insert_with(Vec::new)
                    .push(entry);
            }
        }
        depth_grouped
    } else {
        rows
    };
    // dbg!(&grouped_rows);

    // Print status table
    let mut dir_keys: Vec<&DirectoryEntry> = grouped_rows.keys().collect();
    dir_keys.sort();

    for key in dir_keys {
        let mut statuses = grouped_rows[key]
            .iter()
            .filter(|status| !(status.local_status.is_none() && !all))
            .cloned()
            .collect::<Vec<_>>();

        if statuses.is_empty() {
            continue;
        }

        // Sort the statuses by filename
        statuses.sort_by(|a, b| a.name.cmp(&b.name));

        let display_key = if key.path.is_empty() {
            ".".to_string()
        } else {
            key.display().to_string()
        };
        let prettier_key = if color {
            display_key.bold().to_string()
        } else {
            display_key.to_string()
        };
        println!("[{}]", prettier_key);
        let file_counts =
            get_counts(&statuses, has_remote_info).expect("Internal error: get_counts().");
        file_counts.pretty_print(short);
        println!();
    }
}

pub fn print_fixed_width_status(
    rows: BTreeMap<DirectoryEntry, Vec<StatusEntry>>,
    nspaces: Option<usize>,
    indent: Option<usize>,
    color: bool,
    all: bool,
) {
    let indent = indent.unwrap_or(0);
    let nspaces = nspaces.unwrap_or(6);
    let abbrev = Some(8);

    // get the max number of columns (in case ragged)
    let max_cols = rows
        .values()
        .flat_map(|v| v.iter())
        .map(|entry| entry.columns(abbrev).len())
        .max()
        .unwrap_or(0);

    let mut max_lengths = vec![0; max_cols];

    // compute max lengths across all rows
    for status in rows.values().flat_map(|v| v.iter()) {
        let cols = status.columns(abbrev);
        for (i, col) in cols.iter().enumerate() {
            max_lengths[i] = max_lengths[i].max(col.len());
        }
    }

    // print status table
    let mut dir_keys: Vec<&DirectoryEntry> = rows.keys().collect();
    dir_keys.sort();

    for key in dir_keys {
        let mut statuses = rows[key].clone();
        // Sort by filename
        statuses.sort_by(|a, b| a.name.cmp(&b.name));

        let display_key = if key.path.is_empty() {
            ".".to_string()
        } else {
            key.display().to_string()
        };
        let prettier_key = if color {
            display_key.bold().to_string()
        } else {
            display_key.to_string()
        };
        println!("[{}]", prettier_key);

        // Print the rows with the correct widths
        for status in statuses {
            if status.local_status.is_none() && !all {
                // ignore things that aren't in the manifest, unless --all
                continue;
            }
            let cols = status.columns(abbrev);
            let mut fixed_row = Vec::new();
            for (i, col) in cols.iter().enumerate() {
                // push a fixed-width column to vector
                let spacer = if i == 0 { " " } else { "" };
                let fixed_col = format!("{}{:width$}", spacer, col, width = max_lengths[i]);
                fixed_row.push(fixed_col);
            }
            let spacer = " ".repeat(nspaces);
            let line = fixed_row.join(&spacer);
            let status_line = if color {
                status.color(line)
            } else {
                line.to_string()
            };
            println!("{}{}", " ".repeat(indent), status_line);
        }
        println!();
    }
}

/* fn organize_by_dir(rows: Vec<StatusEntry>) -> BTreeMap<String, Vec<StatusEntry>> {
let mut dir_map: BTreeMap<String, Vec<StatusEntry>> = BTreeMap::new();

for entry in rows {
if let Some(cols) = &entry.cols {
if let Some(first_elem) = cols.first() {
let path = Path::new(first_elem);
if let Some(parent_path) = path.parent() {
let parent_dir = parent_path.to_string_lossy().into_owned();
dir_map.entry(parent_dir).or_default().push(entry);
}
}
}
}
dir_map
}
*/

pub fn pluralize<T: Into<u64>>(count: T, noun: &str) -> String {
    let count = count.into();
    if count == 1 {
        format!("{} {}", count, noun)
    } else {
        format!("{} {}s", count, noun)
    }
}

#[derive(Debug, Default)]
struct FileCounts {
    local: u64,
    local_current: u64,
    local_modified: u64,
    remote: u64,
    both: u64,
    total: u64,
    messy: u64,
}

impl FileCounts {
    pub fn pretty_print(&self, short: bool) {
        if short {
            // Only show categories that have files
            let mut parts = Vec::new();
            if self.local > 0 {
                if self.local_modified > 0 {
                    parts.push(format!(
                        "{} local ({} modified)",
                        self.local.to_string().green(),
                        self.local_modified.to_string().red()
                    ));
                } else {
                    parts.push(format!("{} local", self.local.to_string().green()));
                }
            }
            if self.remote > 0 {
                parts.push(format!("{} remote-only", self.remote.to_string().yellow()));
            }
            if self.both > 0 {
                parts.push(format!("{} synced", self.both.to_string().cyan()));
            }
            if self.messy > 0 {
                parts.push(format!("{} messy", self.messy.to_string().red()));
            }

            if parts.is_empty() {
                println!("no files");
            } else {
                println!(
                    "{} ({})",
                    parts.join(", "),
                    format!("total: {}", self.total).bold()
                );
            }
        } else {
            println!("{}", format!("  {} files total", self.total).bold());
            if self.both > 0 {
                println!("  ✓ {} synced with remote", self.both.to_string().cyan());
            }
            if self.local > 0 {
                let status = if self.local_modified > 0 {
                    format!(
                        " ({} current, {} modified)",
                        self.local_current.to_string().green(),
                        self.local_modified.to_string().red()
                    )
                } else {
                    format!(" (all current)")
                };
                println!(
                    "  + {} local only{}",
                    self.local.to_string().green(),
                    status
                );
            }
            if self.remote > 0 {
                println!("  - {} remote only", self.remote.to_string().yellow());
            }
            if self.messy > 0 {
                println!("  ! {} messy", self.messy.to_string().red());
            }
        }
    }
}

fn get_counts(files: &Vec<StatusEntry>, has_remote_info: bool) -> Result<FileCounts> {
    let mut local = 0;
    let mut local_current = 0;
    let mut local_modified = 0;
    let mut remote = 0;
    let mut both = 0;
    let mut total = 0;
    let mut messy = 0;

    for file in files {
        total += 1;

        if !has_remote_info {
            // When we don't have remote info, everything local is just local
            if let Some(status) = &file.local_status {
                local += 1;
                match status {
                    LocalStatusCode::Current => local_current += 1,
                    LocalStatusCode::Modified => local_modified += 1,
                    _ => messy += 1,
                }
            }
            continue;
        }

        match (&file.local_status, &file.remote_status, &file.tracked) {
            (None, None, _) => {
                return Err(anyhow!(
                    "Internal Error: get_counts found a file with both local/remote set to None."
                ));
            }
            // Local files (including those with NotExists remote status)
            (Some(local_status), Some(RemoteStatusCode::NotExists), _)
            | (Some(local_status), None, Some(false))
            | (Some(local_status), None, None) => {
                local += 1;
                match local_status {
                    LocalStatusCode::Current => local_current += 1,
                    LocalStatusCode::Modified => local_modified += 1,
                    _ => messy += 1,
                }
            }
            // Files that exist both locally and remotely
            (Some(_), Some(_), Some(true))
                if matches!(file.remote_status, Some(RemoteStatusCode::Current)) =>
            {
                both += 1;
            }
            // Remote only files
            (None, Some(_), _) => {
                remote += 1;
            }
            // Everything else is messy
            _ => {
                messy += 1;
            }
        }
    }
    Ok(FileCounts {
        local,
        local_current,
        local_modified,
        remote,
        both,
        total,
        messy,
    })
}

impl Add for FileCounts {
    type Output = FileCounts;

    fn add(self, other: FileCounts) -> FileCounts {
        FileCounts {
            local: self.local + other.local,
            local_current: self.local_current + other.local_current,
            local_modified: self.local_modified + other.local_modified,
            remote: self.remote + other.remote,
            both: self.both + other.both,
            total: self.total + other.total,
            messy: self.messy + other.messy,
        }
    }
}

fn get_counts_tree(
    rows: &BTreeMap<String, Vec<StatusEntry>>,
    has_remote_info: bool,
) -> Result<FileCounts> {
    let mut counts = FileCounts::default();
    for files in rows.values() {
        counts = counts + get_counts(files, has_remote_info)?;
    }
    Ok(counts)
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DirectoryEntry {
    path: String,
    remote_name: Option<String>,
}

impl DirectoryEntry {
    fn display(&self) -> String {
        if let Some(remote) = &self.remote_name {
            format!("{} > {}", self.path, remote)
        } else {
            self.path.clone()
        }
    }
}

pub fn print_status(
    rows: BTreeMap<String, Vec<StatusEntry>>,
    remote: Option<&HashMap<String, Remote>>,
    all: bool,
    short: bool,
    has_remote_info: bool,
    depth: Option<usize>,
) {
    println!("{}", "Project data status:".bold());

    // Pass the remote info state to get_counts
    let counts =
        get_counts_tree(&rows, has_remote_info).expect("Internal Error: get_counts() panicked.");

    // Adjust the status message based on whether we have remote info
    if has_remote_info {
        println!(
            "{} local and tracked by a remote ({} only local, {} only remote), {} total.\n",
            pluralize(counts.both, "file"),
            pluralize(counts.local, "file"),
            pluralize(counts.remote, "file"),
            pluralize(counts.total, "file")
        );
    } else {
        println!("{} local files total.\n", pluralize(counts.total, "file"));
    }

    let rows_by_dir: BTreeMap<DirectoryEntry, Vec<StatusEntry>> = match remote {
        Some(remote_map) => {
            let mut new_map = BTreeMap::new();
            for (directory, statuses) in rows {
                let entry = if let Some(remote) = remote_map.get(&directory) {
                    DirectoryEntry {
                        path: directory,
                        remote_name: Some(remote.name().to_string()),
                    }
                } else {
                    DirectoryEntry {
                        path: directory,
                        remote_name: None,
                    }
                };
                new_map.insert(entry, statuses);
            }
            new_map
        }
        None => rows
            .into_iter()
            .map(|(dir, statuses)| {
                (
                    DirectoryEntry {
                        path: dir,
                        remote_name: None,
                    },
                    statuses,
                )
            })
            .collect(),
    };

    if depth.is_some() {
        print_fixed_width_status_short(rows_by_dir, true, all, short, depth, has_remote_info)
    } else {
        print_fixed_width_status(rows_by_dir, None, None, true, all);
    }
}

pub fn format_bytes(size: u64) -> String {
    const BYTES_IN_KB: f64 = 1024.0;
    const BYTES_IN_MB: f64 = BYTES_IN_KB * 1024.0;
    const BYTES_IN_GB: f64 = BYTES_IN_MB * 1024.0;
    const BYTES_IN_TB: f64 = BYTES_IN_GB * 1024.0;
    const BYTES_IN_PB: f64 = BYTES_IN_TB * 1024.0;
    let size = size as f64;

    if size < BYTES_IN_MB {
        format!("{:.2} MB", size / BYTES_IN_KB)
    } else if size < BYTES_IN_GB {
        format!("{:.2} MB", size / BYTES_IN_MB)
    } else if size < BYTES_IN_TB {
        format!("{:.2} GB", size / BYTES_IN_GB)
    } else if size < BYTES_IN_PB {
        format!("{:.2} TB", size / BYTES_IN_TB)
    } else {
        format!("{:.2} PB", size / BYTES_IN_PB)
    }
}

pub fn format_mod_time(mod_time: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration_since_mod = now.signed_duration_since(mod_time);

    // convert chrono::Duration to std::time::Duration
    let std_duration = std::time::Duration::new(duration_since_mod.num_seconds() as u64, 0);

    let formatter = Formatter::new();
    let local_time = mod_time.with_timezone(&Local);
    let timestamp = local_time.format("%Y-%m-%d %l:%M%p").to_string();
    format!("{} ({})", timestamp, formatter.convert(std_duration))
}

pub fn shorten(hash: &str, abbrev: Option<i32>) -> String {
    let n = abbrev.unwrap_or(hash.len() as i32) as usize;
    hash.chars().take(n).collect()
}

pub fn md5_status(
    new_md5: Option<&String>,
    old_md5: Option<&String>,
    abbrev: Option<i32>,
) -> String {
    match (new_md5, old_md5) {
        (Some(new), Some(old)) => {
            if new == old {
                shorten(new, abbrev)
            } else {
                format!("{} → {}", shorten(old, abbrev), shorten(new, abbrev))
            }
        }
        (None, Some(old)) => shorten(old, abbrev),
        _ => "".to_string(),
    }
}
