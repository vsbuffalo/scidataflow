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
use super::status::StatusDisplayOptions;

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
    options: &StatusDisplayOptions,
) {
    let depth = options.get_depth();
    // If depth is provided, reorganize the data based on the specified depth
    let grouped_rows: BTreeMap<DirectoryEntry, Vec<StatusEntry>> = if let Some(depth) = depth {
        let mut depth_grouped: BTreeMap<DirectoryEntry, Vec<StatusEntry>> = BTreeMap::new();
        for (dir_entry, entries) in rows {
            for entry in entries {
                let base_dir = get_dir_at_depth(&dir_entry.path, &entry.name, depth);
                depth_grouped
                    .entry(DirectoryEntry {
                        path: base_dir,
                        remote_name: dir_entry.remote_name.clone(),
                    })
                    .or_default()
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
            .filter(|status| status.local_status.is_some() || options.all)
            .cloned()
            .collect::<Vec<_>>();

        if statuses.is_empty() {
            continue;
        }

        // TODO: we should consolidate code between this and
        // print_fixed_width_status_short.
        if !options.time {
            // Sort the statuses by filename
            statuses.sort_by(|a, b| a.name.cmp(&b.name));
        } else {
            // Sort the statuses by timestamp
            statuses.sort_by(|a, b| b.local_mod_time.cmp(&a.local_mod_time));
        }

        if options.reverse {
            statuses.reverse();
        }

        let display_key = if key.path.is_empty() {
            ".".to_string()
        } else {
            key.display().to_string()
        };
        let prettier_key = if !options.no_color {
            display_key.bold().to_string()
        } else {
            display_key.to_string()
        };
        println!("[{}]", prettier_key);
        let file_counts =
            get_counts(&statuses, options.remotes).expect("Internal error: get_counts().");
        file_counts.pretty_print(options.short, !options.no_color);
        println!();
    }
}

pub fn print_fixed_width_status(
    rows: BTreeMap<DirectoryEntry, Vec<StatusEntry>>,
    nspaces: Option<usize>,
    indent: Option<usize>,
    options: &StatusDisplayOptions,
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
        if !options.time {
            // Sort the statuses by filename
            statuses.sort_by(|a, b| a.name.cmp(&b.name));
        } else {
            // Sort the statuses by timestamp
            statuses.sort_by(|a, b| b.local_mod_time.cmp(&a.local_mod_time));
        }

        if options.reverse {
            statuses.reverse();
        }

        let display_key = if key.path.is_empty() {
            ".".to_string()
        } else {
            key.display().to_string()
        };
        let prettier_key = if !options.no_color {
            display_key.bold().to_string()
        } else {
            display_key.to_string()
        };
        println!("[{}]", prettier_key);

        // Print the rows with the correct widths
        for status in statuses {
            if status.local_status.is_none() && !options.all {
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
            let status_line = if !options.no_color {
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
    local: u64,            // Total local files
    local_current: u64,    // Files that match their manifest MD5
    local_modified: u64,   // Files that differ from manifest MD5
    local_deleted: u64,    // Files in manifest but not on disk
    remote: u64,           // Files only on remote
    both: u64,             // Files synced between local and remote
    remote_different: u64, // Files where local matches manifest but differs from remote
    local_messy: u64,      // Files where local differs from both manifest and remote (MessyLocal)
    total: u64,            // Total number of files
}

impl FileCounts {
    pub fn pretty_print(&self, short: bool, color: bool) {
        // Helper closure to conditionally apply color
        let colorize = |text: String, color_fn: fn(String) -> ColoredString| -> String {
            if color {
                color_fn(text).to_string()
            } else {
                text
            }
        };

        if short {
            let mut parts = Vec::new();
            if self.local > 0 {
                let mut local_str = format!("{} local", self.local);
                local_str = colorize(local_str, |s| s.green());

                let mut issues = Vec::new();
                if self.local_modified > 0 {
                    issues.push(format!(
                        "{} modified",
                        colorize(self.local_modified.to_string(), |s| s.red())
                    ));
                }
                if self.local_deleted > 0 {
                    issues.push(format!(
                        "{} deleted",
                        colorize(self.local_deleted.to_string(), |s| s.yellow())
                    ));
                }
                if !issues.is_empty() {
                    local_str = format!("{} ({})", local_str, issues.join(", "));
                }
                parts.push(local_str);
            }
            if self.remote > 0 {
                parts.push(format!(
                    "{} remote-only",
                    colorize(self.remote.to_string(), |s| s.yellow())
                ));
            }
            if self.both > 0 {
                parts.push(format!(
                    "{} synced",
                    colorize(self.both.to_string(), |s| s.cyan())
                ));
            }
            if self.remote_different > 0 {
                parts.push(format!(
                    "{} differ from remote",
                    colorize(self.remote_different.to_string(), |s| s.yellow())
                ));
            }
            if self.local_messy > 0 {
                parts.push(format!(
                    "{} needs update",
                    colorize(self.local_messy.to_string(), |s| s.red())
                ));
            }
            if parts.is_empty() {
                println!("no files");
            } else {
                println!(
                    "{} ({})",
                    parts.join(", "),
                    colorize(format!("total: {}", self.total), |s| s.bold())
                );
            }
        } else {
            println!(
                "{}",
                colorize(format!("  {} files total", self.total), |s| s.bold())
            );
            if self.both > 0 {
                println!(
                    "  ✓ {} synced with remote",
                    colorize(self.both.to_string(), |s| s.cyan())
                );
            }
            if self.local > 0 {
                let mut status_parts = Vec::new();
                if self.local_current > 0 {
                    status_parts.push(format!(
                        "{} current",
                        colorize(self.local_current.to_string(), |s| s.green())
                    ));
                }
                if self.local_modified > 0 {
                    status_parts.push(format!(
                        "{} modified",
                        colorize(self.local_modified.to_string(), |s| s.red())
                    ));
                }
                if self.local_deleted > 0 {
                    status_parts.push(format!(
                        "{} deleted",
                        colorize(self.local_deleted.to_string(), |s| s.yellow())
                    ));
                }
                let status = if !status_parts.is_empty() {
                    format!(" ({})", status_parts.join(", "))
                } else {
                    String::from(" (all current)")
                };
                println!(
                    "  + {} local only{}",
                    colorize(self.local.to_string(), |s| s.green()),
                    status
                );
            }
            if self.remote > 0 {
                println!(
                    "  - {} remote only",
                    colorize(self.remote.to_string(), |s| s.yellow())
                );
            }
            if self.remote_different > 0 {
                println!(
                    "  ! {} differ from remote",
                    colorize(self.remote_different.to_string(), |s| s.yellow())
                );
            }
            if self.local_messy > 0 {
                println!(
                    "  ! {} need update",
                    colorize(self.local_messy.to_string(), |s| s.red())
                );
            }
        }
    }
}

fn get_counts(files: &Vec<StatusEntry>, has_remote_info: bool) -> Result<FileCounts> {
    let mut counts = FileCounts::default();

    for file in files {
        counts.total += 1;
        if !has_remote_info {
            // When we don't have remote info, only track local status
            if let Some(status) = &file.local_status {
                match status {
                    LocalStatusCode::Current => {
                        counts.local += 1;
                        counts.local_current += 1;
                    }
                    LocalStatusCode::Modified => {
                        counts.local += 1;
                        counts.local_modified += 1;
                    }
                    LocalStatusCode::Deleted => {
                        counts.local_deleted += 1;
                    }
                    LocalStatusCode::Invalid => {
                        counts.local_messy += 1;
                    }
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
            // Local files that match manifest but have no remote or aren't tracked
            (Some(LocalStatusCode::Current), Some(RemoteStatusCode::NotExists), _)
            | (Some(LocalStatusCode::Current), None, Some(false))
            | (Some(LocalStatusCode::Current), None, None) => {
                counts.local += 1;
                counts.local_current += 1;
            }
            // Modified local files that have no remote or aren't tracked
            (Some(LocalStatusCode::Modified), Some(RemoteStatusCode::NotExists), _)
            | (Some(LocalStatusCode::Modified), None, Some(false))
            | (Some(LocalStatusCode::Modified), None, None) => {
                counts.local += 1;
                counts.local_modified += 1;
            }
            // Deleted local files
            (Some(LocalStatusCode::Deleted), _, _) => {
                counts.local_deleted += 1;
            }
            // Files that are perfectly synced (local matches manifest matches remote)
            (Some(LocalStatusCode::Current), Some(RemoteStatusCode::Current), Some(true)) => {
                counts.both += 1;
            }
            // Local file matches manifest but differs from remote
            (Some(LocalStatusCode::Current), Some(RemoteStatusCode::Different), Some(true)) => {
                counts.remote_different += 1;
            }
            // Local file exists but doesn't match manifest or remote
            (Some(_), Some(RemoteStatusCode::MessyLocal), _) => {
                counts.local_messy += 1;
            }
            // Files that only exist on remote
            (None, Some(RemoteStatusCode::Current), _)
            | (None, Some(RemoteStatusCode::Exists), _)
            | (None, Some(RemoteStatusCode::NoLocal), _) => {
                counts.remote += 1;
            }
            // Remote file exists but we can't compare MD5s
            (Some(LocalStatusCode::Current), Some(RemoteStatusCode::Exists), Some(true)) => {
                counts.remote_different += 1;
            }
            // Everything else is counted as messy
            _ => {
                counts.local_messy += 1;
            }
        }
    }
    Ok(counts)
}

impl Add for FileCounts {
    type Output = FileCounts;

    fn add(self, other: FileCounts) -> FileCounts {
        FileCounts {
            local: self.local + other.local,
            local_current: self.local_current + other.local_current,
            local_modified: self.local_modified + other.local_modified,
            local_deleted: self.local_deleted + other.local_deleted,
            remote: self.remote + other.remote,
            both: self.both + other.both,
            remote_different: self.remote_different + other.remote_different,
            local_messy: self.local_messy + other.local_messy,
            total: self.total + other.total,
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
    options: &StatusDisplayOptions,
) {
    println!("{}", "Project data status:".bold());

    // Pass the remote info state to get_counts
    let counts =
        get_counts_tree(&rows, options.remotes).expect("Internal Error: get_counts() panicked.");

    // Adjust the status message based on whether we have remote info
    if options.remotes {
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

    if options.get_depth().is_some() {
        print_fixed_width_status_short(rows_by_dir, options)
    } else {
        print_fixed_width_status(rows_by_dir, None, None, options);
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
