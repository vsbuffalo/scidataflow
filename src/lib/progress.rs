use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use std::thread;
use anyhow::{anyhow,Result};

// these are separated since some APIs don't overload
// indicatif bars, but take the same primitives.
pub const DEFAULT_PROGRESS_STYLE: &str = "{spinner:.green} [{bar:40.green/white}] {pos:>}/{len} ({percent}%) eta {eta_precise:.green} {msg}";
pub const DEFAULT_PROGRESS_INC: &str = "=>";

pub fn default_progress_style() -> Result<ProgressStyle, anyhow::Error> {
    let style = ProgressStyle::default_bar()
        .progress_chars(DEFAULT_PROGRESS_INC)
        .template(DEFAULT_PROGRESS_STYLE)?;
    Ok(style)
}

pub struct Progress {
    pub bar: ProgressBar,
    spinner: thread::JoinHandle<()>
}

impl Progress {
    pub fn new(len: u64) -> Result<Progress> {
        let bar = ProgressBar::new(len as u64);
        bar.set_style(default_progress_style()?);

        let bar_clone = bar.clone();
        let spinner = thread::spawn(move || {
            loop {
                bar_clone.tick();
                thread::sleep(Duration::from_millis(20));
            }
        });
        Ok(Progress { bar, spinner })
    } 
}
