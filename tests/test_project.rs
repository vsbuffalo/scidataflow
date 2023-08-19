use std::fs;
use std::env;
use std::mem;
use tempfile::TempDir;
#[allow(unused_imports)]
use log::{info, trace, debug};

mod common;
use common::TestEnvironment;

use sciflow::lib::project::Project;


const CONFIG_YAML: &str = include_str!("test_data/project_structure.yaml");

#[cfg(test)]
mod tests {
    use log::info;
    use sciflow::lib::project::Project;
    use crate::CONFIG_YAML;
    use super::TestEnvironment;
    use std::fs;

    #[test]
    fn test_init() {
        env_logger::init();
        let test_env = TestEnvironment::new().expect("Error creating test environment.");
        test_env.build_project_directories(CONFIG_YAML);
    }
}

