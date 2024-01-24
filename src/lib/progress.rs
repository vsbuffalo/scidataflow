use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

// these are separated since some APIs don't overload
// indicatif bars, but take the same primitives.
pub const DEFAULT_PROGRESS_STYLE: &str = "{spinner:.green} [{bar:40.green/white}] {pos:>}/{len} ({percent}%) eta {eta_precise:.green} {msg}";
pub const DEFAULT_PROGRESS_INC: &str = "=> ";

pub fn default_progress_style() -> Result<ProgressStyle, anyhow::Error> {
    let style = ProgressStyle::default_bar()
        .progress_chars(DEFAULT_PROGRESS_INC)
        .template(DEFAULT_PROGRESS_STYLE)?;
    Ok(style)
}

pub struct Progress {
    pub bar: ProgressBar,
    stop_spinner: Sender<()>,
    #[allow(dead_code)]
    spinner: Option<thread::JoinHandle<()>>,
}

impl Progress {
    pub fn new(len: u64) -> Result<Progress> {
        let bar = ProgressBar::new(len);
        bar.set_style(default_progress_style()?);

        let (tx, rx): (Sender<()>, Receiver<()>) = mpsc::channel();

        let bar_clone = bar.clone();
        let spinner = thread::spawn(move || loop {
            if rx.try_recv().is_ok() {
                break;
            }
            bar_clone.tick();
            thread::sleep(Duration::from_millis(20));
        });
        Ok(Progress {
            bar,
            stop_spinner: tx,
            spinner: Some(spinner),
        })
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.stop_spinner.send(()).unwrap();
        if let Some(spinner) = self.spinner.take() {
            spinner.join().expect("Failed to join spinner thread");
        }
    }
}
