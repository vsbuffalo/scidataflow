use clap::Parser;

/// Status display options
#[derive(Parser, Debug)]
pub struct StatusDisplayOptions {
    /// Show remotes status (requires network).
    #[arg(short = 'm', long)]
    pub remotes: bool,

    /// Show statuses of all files, including those on remote(s)
    /// but not in the manifest.
    #[arg(short, long)]
    pub all: bool,

    /// Don't print status with terminal colors.
    #[arg(long)]
    pub no_color: bool,

    /// A more terse summary, with --depth 2.
    #[arg(short, long)]
    pub short: bool,

    /// Depth to summarize over.
    #[arg(short, long)]
    depth: Option<usize>,

    /// Sort by time, showing the most recently modified files at
    /// the top.
    #[arg(short, long)]
    pub time: bool,

    /// Reverse file order (if --time set, will show the files
    /// with the oldest modification time at the top; otherwise
    /// it will list files in reverse lexicographic order).
    #[arg(short, long)]
    pub reverse: bool,
}

impl StatusDisplayOptions {
    pub fn get_depth(&self) -> Option<usize> {
        if self.short {
            // --short includes
            return Some(2);
        }
        self.depth
    }
}
