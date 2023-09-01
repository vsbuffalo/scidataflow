use anyhow::{anyhow,Result,Context};
use std::path::PathBuf;
use reqwest::Url;

use trauma::downloader::{DownloaderBuilder,StyleOptions,ProgressBarOpts};
use trauma::download::Download;

use crate::lib::progress::{DEFAULT_PROGRESS_STYLE, DEFAULT_PROGRESS_INC};
use crate::lib::utils::pluralize;

pub struct Downloads {
    pub list: Vec<Download>,
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

impl Downloads {
    pub fn new() -> Self {
        let list = Vec::new();
        Downloads { list }
    }

    pub fn add<T: Downloadable>(&mut self, item: T, filename: Option<&str>,
                                overwrite: bool) -> Result<Option<&Download>> {
        let url = item.to_url()?;

        let resolved_filename = match filename {
            Some(name) => name.to_string(),
            None => {
                url.path_segments()
                    .ok_or_else(|| anyhow::anyhow!("Error parsing URL."))?
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("Error getting filename from download URL."))?
                    .to_string()
            }
        };

        let file_path = PathBuf::from(&resolved_filename);
        if file_path.exists() && !overwrite {
            return Ok(None);
        }
 
        let download = Download { url, filename: resolved_filename };
        self.list.push(download);
        Ok(Some(self.list.last().ok_or(anyhow::anyhow!("Failed to add download"))?))
    }

    pub fn default_style(&self) -> Result<StyleOptions> {
        let style = ProgressBarOpts::new(
            Some(DEFAULT_PROGRESS_STYLE.to_string()),
            Some(DEFAULT_PROGRESS_INC.to_string()),
            true, true);

        let style_clone = style.clone();
        Ok(StyleOptions::new(style, style_clone))
    }


    pub async fn retrieve(&self, success_status: Option<&str>, 
                              no_downloads_message: Option<&str>) -> Result<()> {
        let downloads = &self.list;
        let total_files = downloads.len();
        if !downloads.is_empty() { 
            let downloader = DownloaderBuilder::new()
                .style_options(self.default_style()?)
                .build();
            downloader.download(&downloads).await;
            println!("Downloaded {}.", pluralize(total_files as u64, "file"));
            for download in downloads {
                if let Some(msg) = success_status {
                    let filename = PathBuf::from(&download.filename);
                    let name_str = filename.file_name().ok_or(anyhow!("Internal Error: could not extract filename from download"))?;
                    //println!(" - {}", name_str.to_string_lossy());
                    println!("{}", msg.replace("{}", &name_str.to_string_lossy()));
                }
            }
        } else {
            if no_downloads_message.is_some() {
                println!("{}", no_downloads_message.unwrap_or(""));
            }
        }
        Ok(())
    }
}
