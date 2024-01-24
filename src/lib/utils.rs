use anyhow::{anyhow, Result};
use chrono::{Local, Utc};
use colored::*;
#[allow(unused_imports)]
use log::{debug, info, trace};
use md5::Context;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use timeago::Formatter;

use crate::lib::data::StatusEntry;
use crate::lib::remote::Remote;

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
/*
   pub fn print_fixed_width(rows: HashMap<String, Vec<StatusEntry>>, nspaces: Option<usize>, indent: Option<usize>, color: bool) {
   let indent = indent.unwrap_or(0);
   let nspaces = nspaces.unwrap_or(6);

   let max_cols = rows.values()
   .flat_map(|v| v.iter())
   .filter_map(|entry| {
   match &entry.cols {
   None => None,
   Some(cols) => Some(cols.len())
   }
   })
   .max()
   .unwrap_or(0);

   let mut max_lengths = vec![0; max_cols];

// compute max lengths across all rows
for entry in rows.values().flat_map(|v| v.iter()) {
if let Some(cols) = &entry.cols {
for (i, col) in cols.iter().enumerate() {
max_lengths[i] = max_lengths[i].max(col.width());
}
}
}
// print status table
let mut keys: Vec<&String> = rows.keys().collect();
keys.sort();
for (key, value) in &rows {
let pretty_key = if color { key.bold().to_string() } else { key.clone() };
println!("[{}]", pretty_key);

// Print the rows with the correct widths
for row in value {
let mut fixed_row = Vec::new();
let tracked = &row.tracked;
let local_status = &row.local_status;
let remote_status = &row.remote_status;
if let Some(cols) = &row.cols {
for (i, col) in cols.iter().enumerate() {
// push a fixed-width column to vector
let fixed_col = format!("{:width$}", col, width = max_lengths[i]);
fixed_row.push(fixed_col);
}
}
let spacer = " ".repeat(nspaces);
let status_line = fixed_row.join(&spacer);
println!("{}{}", " ".repeat(indent), status_line);
}
println!();
}
}
*/
// More specialized version of print_fixed_width() for statuses.
// Handles coloring, manual annotation, etc
pub fn print_fixed_width_status(
    rows: BTreeMap<String, Vec<StatusEntry>>,
    nspaces: Option<usize>,
    indent: Option<usize>,
    color: bool,
    all: bool,
) {
    //debug!("rows: {:?}", rows);
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
            max_lengths[i] = max_lengths[i].max(col.len()); // Assuming col is a string
        }
    }

    // print status table
    let mut dir_keys: Vec<&String> = rows.keys().collect();
    dir_keys.sort();
    for key in dir_keys {
        let statuses = &rows[key];
        let pretty_key = if key.is_empty() { "." } else { key };
        let prettier_key = if color {
            pretty_key.bold().to_string()
        } else {
            pretty_key.to_string()
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

struct FileCounts {
    local: u64,
    remote: u64,
    both: u64,
    total: u64,
    #[allow(dead_code)]
    messy: u64,
}

fn get_counts(rows: &BTreeMap<String, Vec<StatusEntry>>) -> Result<FileCounts> {
    let mut local = 0;
    let mut remote = 0;
    let mut both = 0;
    let mut total = 0;
    let mut messy = 0;
    for files in rows.values() {
        for file in files {
            total += 1;
            match (&file.local_status, &file.remote_status, &file.tracked) {
                (None, None, _) => {
                    return Err(anyhow!("Internal Error: get_counts found a file with both local/remote set to None."));
                }
                (None, Some(_), None) => {
                    remote += 1;
                }
                (Some(_), None, Some(false)) => {
                    local += 1;
                }
                (Some(_), None, None) => {
                    local += 1;
                }
                (None, Some(_), Some(true)) => {
                    remote += 1;
                }
                (None, Some(_), Some(false)) => {
                    local += 1;
                }
                (Some(_), Some(_), Some(true)) => {
                    both += 1;
                }
                (Some(_), Some(_), Some(false)) => {
                    messy += 1;
                }
                (Some(_), None, Some(true)) => {
                    remote += 1;
                }
                (Some(_), Some(_), None) => {
                    messy += 1;
                }
            }
        }
    }
    Ok(FileCounts {
        local,
        remote,
        both,
        total,
        messy,
    })
}

pub fn print_status(
    rows: BTreeMap<String, Vec<StatusEntry>>,
    remote: Option<&HashMap<String, Remote>>,
    all: bool,
) {
    println!("{}", "Project data status:".bold());
    let counts = get_counts(&rows).expect("Internal Error: get_counts() panicked.");
    println!(
        "{} local and tracked by a remote ({} only local, {} only remote), {} total.\n",
        pluralize(counts.both, "file"),
        pluralize(counts.local, "file"),
        pluralize(counts.remote, "file"),
        //pluralize(counts.messy as u64, "file"),
        pluralize(counts.total, "file")
    );

    // this brings the remote name (if there is a corresponding remote) into
    // the key, so the linked remote can be displayed in the status
    let rows_by_dir: BTreeMap<String, Vec<StatusEntry>> = match remote {
        Some(remote_map) => {
            let mut new_map = BTreeMap::new();
            for (directory, statuses) in rows {
                if let Some(remote) = remote_map.get(&directory) {
                    let new_key = format!("{} > {}", directory, remote.name());
                    new_map.insert(new_key, statuses);
                } else {
                    new_map.insert(directory, statuses);
                }
            }
            new_map
        }
        None => rows,
    };

    print_fixed_width_status(rows_by_dir, None, None, true, all);
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

pub fn shorten(hash: &String, abbrev: Option<i32>) -> String {
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
