use anyhow::{anyhow, Context, Result};
use reqwest::Url;
use std::fs;
use std::path::PathBuf;

use trauma::download::Download;
use trauma::downloader::{DownloaderBuilder, ProgressBarOpts, StyleOptions};

use crate::lib::progress::{DEFAULT_PROGRESS_INC, DEFAULT_PROGRESS_STYLE};
use crate::lib::utils::pluralize;

pub struct Downloads {
    pub queue: Vec<Download>,
}

pub trait Downloadable {
    fn to_url(self) -> Result<Url>;
}

impl Downloadable for String {
    fn to_url(self) -> Result<Url> {
        let url = Url::parse(&self).context(format!("Download URL '{}' is not valid.", &self))?;
        Ok(url)
    }
}

impl Downloadable for Url {
    fn to_url(self) -> Result<Url> {
        Ok(self)
    }
}

impl Default for Downloads {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloads {
    pub fn new() -> Self {
        let queue = Vec::new();
        Downloads { queue }
    }

    pub fn add<T: Downloadable>(
        &mut self,
        item: T,
        filename: Option<&str>,
        overwrite: bool,
    ) -> Result<Option<&Download>> {
        let url = item.to_url()?;

        let resolved_filename = match filename {
            Some(name) => name.to_string(),
            None => url
                .path_segments()
                .ok_or_else(|| anyhow::anyhow!("Error parsing URL."))?
                .last()
                .ok_or_else(|| anyhow::anyhow!("Error getting filename from download URL."))?
                .to_string(),
        };

        let file_path = PathBuf::from(&resolved_filename);
        if file_path.exists() && !overwrite {
            return Ok(None);
        }

        let download = Download {
            url,
            filename: resolved_filename,
        };
        self.queue.push(download);
        Ok(Some(
            self.queue
                .last()
                .ok_or(anyhow::anyhow!("Failed to add download"))?,
        ))
    }

    pub fn default_style(&self) -> Result<StyleOptions> {
        let style = ProgressBarOpts::new(
            Some(DEFAULT_PROGRESS_STYLE.to_string()),
            Some(DEFAULT_PROGRESS_INC.to_string()),
            true,
            true,
        );

        let style_clone = style.clone();
        Ok(StyleOptions::new(style, style_clone))
    }

    // Retrieve all files in the download queue.
    //
    // Note: if the file is in the queue, at this point it is considered *overwrite safe*.
    // This is because overwrite-safety is checked at Downloads::add(), per-file.
    // The trauma crate does not overwrite files; delete must be done manually here
    // first if it exists.
    pub async fn retrieve(
        &self,
        success_status: Option<&str>,
        no_downloads_message: Option<&str>,
        show_total: bool,
    ) -> Result<()> {
        let downloads = &self.queue;
        let total_files = downloads.len();
        if !downloads.is_empty() {
            // Let's handle the file operations:
            // 1) Move all the files to temporary destinations
            // 2) Create the directory structure if it does not exist.
            let mut temp_files = Vec::new();
            for file in downloads {
                let path = PathBuf::from(&file.filename);
                if path.exists() {
                    // rather than delete, we move the file
                    let temp_file_path = path.with_extension(".tmp");
                    fs::rename(&path, &temp_file_path)?;
                    temp_files.push(temp_file_path);
                }

                // recreate the directory structure if not there
                if let Some(parent_dir) = path.parent() {
                    if !parent_dir.exists() {
                        fs::create_dir_all(parent_dir)?;
                    }
                }
            }

            let downloader = DownloaderBuilder::new()
                .style_options(self.default_style()?)
                .build();

            // download everything
            downloader.download(downloads).await;

            // now remove the temp files
            for temp_file_path in temp_files {
                if temp_file_path.exists() {
                    fs::remove_file(temp_file_path)?;
                }
            }
            if show_total {
                let punc = if total_files > 0 { "." } else { ":" };
                println!(
                    "Downloaded {}{}",
                    pluralize(total_files as u64, "file"),
                    punc
                );
            }
            for download in downloads {
                if let Some(msg) = success_status {
                    let filename = PathBuf::from(&download.filename);
                    let name_str = filename.file_name().ok_or(anyhow!(
                        "Internal Error: could not extract filename from download"
                    ))?;
                    //println!(" - {}", name_str.to_string_lossy());
                    println!("{}", msg.replace("{}", &name_str.to_string_lossy()));
                }
            }
        } else if no_downloads_message.is_some() {
            println!("{}", no_downloads_message.unwrap_or(""));
        }
        Ok(())
    }
}
