use std::collections::HashMap;
use std::os::macos::raw::stat;
use std::path::{Path,PathBuf};
use std::fs::{File};
use std::io::Read;
use md5::{Digest, Context};
use colored::*;
use unicode_width::UnicodeWidthStr;

use crate::data::{StatusEntry, StatusCode};

const BUFFER_SIZE: usize = 4096;


pub fn load_file(path: &PathBuf) -> String {
    let mut file = File::open(&path).expect("unable to open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("unable to read file");
    contents
}

pub fn compute_md5(file_path: &Path) -> Option<String> {
    let mut file = match File::open(file_path) {
        Ok(file) => file,
        Err(_) => return None,
    };

    let mut buffer = [0; BUFFER_SIZE];
    let mut md5 = Context::new();

    loop {
        let bytes_read = match file.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(bytes_read) => bytes_read,
            Err(_) => return None,
        };

        md5.consume(&buffer[..bytes_read]);
    }
    let result = md5.compute();
    Some(format!("{:x}", result))
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

    // print status table
    for (key, value) in &rows {
        let pretty_key = if color { key.bold().to_string() } else { key.clone() };
        println!("[{}]", pretty_key);

        // Print the rows with the correct widths
        for row in value {
            let mut fixed_row = Vec::new();
            let mut code = &row.code;
            for (i, col) in row.cols.iter().enumerate() {
                if i == 1 { // where to put status
                    // include the status
                    let status_msg = match code {
                            StatusCode::Current => "current",
                            StatusCode::Changed => "changed",
                            StatusCode::DiskChanged => "changed",
                            StatusCode::Updated => "updated, not changed",
                            StatusCode::Invalid => "!INVALID!",
                            _ => &col
                        };

                    let status_col = format!("{:width$}", status_msg, width = max_lengths[i]);
                    fixed_row.push(status_col)
                }
                let fixed_col = format!("{:width$}", col, width = max_lengths[i]);
                fixed_row.push(fixed_col);
            }
            let spacer = " ".repeat(nspaces);

            // color row
            let status_line = fixed_row.join(&spacer);
            let status_line = match code {
                    StatusCode::Current => status_line.green().to_string(),
                    StatusCode::Changed => status_line.red().to_string(),
                    StatusCode::DiskChanged => status_line.bright_red().to_string(),
                    StatusCode::Updated => status_line.yellow().to_string(),
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

pub fn print_status(rows: Vec<StatusEntry>) {
    println!("{}", "Project data status:".bold());
    println!("{} data file{} tracked.\n", rows.len(), if rows.len() > 1 {"s"} else {""});
    let rows_by_dir = organize_by_dir(rows);
    print_fixed_width(rows_by_dir, None, None, false);
}
