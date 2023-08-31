pub mod lib {
    pub mod data;
    pub mod api {
        pub mod dryad;
        pub mod figshare;
        pub mod zenodo;
    }
    pub mod project;
    pub mod download;
    pub mod progress;
    pub mod macros;
    pub mod remote;
    pub mod utils;
    pub mod test_utilities;
}

pub mod logging_setup;
