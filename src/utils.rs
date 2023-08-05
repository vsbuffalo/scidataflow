use std::collections::HashMap;
use anyhow::{anyhow,Result};
use chrono::{Duration, Utc,Local};
use timeago::Formatter;
use std::path::{Path,PathBuf};
use std::fs::{File};
use std::io::Read;
use md5::{Context};
use log::{info, trace, debug};
use colored::*;
use unicode_width::UnicodeWidthStr;

use crate::data::{StatusEntry, StatusCode};
use super::remote::{Remote};

const BUFFER_SIZE: usize = 4096;


pub fn load_file(path: &PathBuf) -> String {
    let mut file = File::open(&path).expect("unable to open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("unable to read file");
    contents
}

/// Compute the MD5 of a file returning None if the file is empty.
pub fn compute_md5(file_path: &Path) -> Result<Option<String>> {
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
            Err(e) => return Err(anyhow!("I/O reading file!".to_string())),
        };

        md5.consume(&buffer[..bytes_read]);
    }
    
    let result = md5.compute();
    Ok(Some(format!("{:x}", result)))
}

pub fn print_fixed_width(rows: HashMap<String, Vec<StatusEntry>>, nspaces: Option<usize>, indent: Option<usize>, color: bool) {
    let indent = indent.unwrap_or(0);
    let nspaces = nspaces.unwrap_or(6);

    let max_cols = rows.values()
        .flat_map(|v| v.iter())
        .map(|entry| entry.cols.len())
        .max().unwrap_or(0);
    let mut max_lengths = vec![0; max_cols];

    // compute max lengths across all rows
    for entry in rows.values().flat_map(|v| v.iter()) {
        for (i, col) in entry.cols.iter().enumerate() {
            max_lengths[i] = max_lengths[i].max(col.width());
        }
    }
    // debug!("max_lengths: {:?}", max_lengths);

    // print status table
    let mut keys: Vec<&String> = rows.keys().collect();
    keys.sort();
    for (key, value) in &rows {
        let pretty_key = if color { key.bold().to_string() } else { key.clone() };
        println!("[{}]", pretty_key);

        // Print the rows with the correct widths
        for row in value {
            let mut fixed_row = Vec::new();
            let status = &row.status;
            for (i, col) in row.cols.iter().enumerate() {
                // push a fixed-width column to vector
                let fixed_col = format!("{:width$}", col, width = max_lengths[i]);
                fixed_row.push(fixed_col);
            }
            let spacer = " ".repeat(nspaces);

            // color row
            let status_line = fixed_row.join(&spacer);
            let status_line = match status {
                    StatusCode::Current => status_line.green().to_string(),
                    StatusCode::Changed => status_line.red().to_string(),
                    _ => status_line
            };
            println!("{}{}", " ".repeat(indent), status_line);
        }
        println!();
    }
}

fn organize_by_dir(rows: Vec<StatusEntry>) -> HashMap<String, Vec<StatusEntry>> {
    let mut dir_map: HashMap<String, Vec<StatusEntry>> = HashMap::new();

    for entry in rows {
        if let Some(first_elem) = entry.cols.first() {
            let path = Path::new(first_elem);
            if let Some(parent_path) = path.parent() {
                let parent_dir = parent_path.to_string_lossy().into_owned();
                dir_map.entry(parent_dir).or_default().push(entry);
            }
        }
    }
    dir_map
}

pub fn print_status(rows: Vec<StatusEntry>, remote: Option<&HashMap<PathBuf,Remote>>) {
    println!("{}", "Project data status:".bold());
    println!("{} data file{} tracked.\n", rows.len(), if rows.len() > 1 {"s"} else {""});

    let organized_rows = organize_by_dir(rows);

    let rows_by_dir: HashMap<String, Vec<StatusEntry>> = match remote {
        Some(remote_map) => {
            let mut new_map = HashMap::new();
            for (key, value) in organized_rows {
                if let Some(remote) = remote_map.get(&PathBuf::from(&key)) {
                    let new_key = format!("{} > {}", key, remote.name());
                    new_map.insert(new_key, value);
                } else {
                    new_map.insert(key, value);
                }
            }
            new_map
        },
        None => organized_rows,
    };

    print_fixed_width(rows_by_dir, None, None, true);
}

pub fn format_bytes(size: u64) -> String {
    const BYTES_IN_KB: f64 = 1024.0;
    const BYTES_IN_MB: f64 = BYTES_IN_KB * 1024.0;
    const BYTES_IN_GB: f64 = BYTES_IN_MB * 1024.0;
    const BYTES_IN_TB: f64 = BYTES_IN_GB * 1024.0;
    const BYTES_IN_PB: f64 = BYTES_IN_TB * 1024.0;
    let size = size as f64;

    if size < BYTES_IN_MB {
        return format!("{:.2} MB", size / BYTES_IN_KB);
    } else if size < BYTES_IN_GB {
        return format!("{:.2} MB", size / BYTES_IN_MB);
    } else if size < BYTES_IN_TB {
        return format!("{:.2} GB", size / BYTES_IN_GB);
    } else if size < BYTES_IN_PB {
        return format!("{:.2} TB", size / BYTES_IN_TB);
    } else {
        return format!("{:.2} PB", size / BYTES_IN_PB);
    }
}


pub fn format_mod_time(mod_time: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration_since_mod = now.signed_duration_since(mod_time);
    
    // convert chrono::Duration to std::time::Duration
    let std_duration = std::time::Duration::new(
        duration_since_mod.num_seconds() as u64,
        0
    );
    
    let formatter = Formatter::new();
    let local_time = mod_time.with_timezone(&Local);
    let timestamp = local_time.format("%Y-%m-%d %l:%M%p").to_string();
    format!("{} ({})", formatter.convert(std_duration), timestamp)
}
