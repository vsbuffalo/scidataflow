///! Testing Utility Functions

use anyhow::{anyhow,Result};
use log::info;
use std::env;
use rand::Rng;
use std::path::{Path,PathBuf};
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;
use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};
use std::io::Read;
use serde_yaml;
use std::collections::HashMap;
use std::io::BufWriter;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::{create_dir_all};


fn generate_random_tsv(file_path: &Path, size: usize, gzip: bool) -> Result<()> {
    let file = File::create(file_path)?;
    let writer: Box<dyn Write> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    let mut writer = BufWriter::new(writer);

    let mut rng = rand::thread_rng();
    let mut bytes_written = 0; // Track the number of bytes written

    while bytes_written < size {
        let value: u32 = rng.gen();
        let line = format!("{}\t{}\t{}\t{}\n", value, value, value, value);
        bytes_written += line.len();
        writer.write_all(line.as_bytes())?;
    }

    writer.flush()?; // Ensure all data is written to the writer

    Ok(())
}

fn parse_yaml(file_path: &str) -> Result<DirectoryConfig> {
    let file = File::open(file_path)?;
    let config: DirectoryConfig = serde_yaml::from_reader(file)?;
    Ok(config)
}

fn generate_directory_structure(config: &DirectoryConfig, base_path: &Path) -> Result<()> {
    for (directory, data_file_fixtures) in &config.directories {
        // make main parent directory
       let directory_path = base_path.join(directory);
        create_dir_all(&directory_path)?;

        for data_file_fixture in data_file_fixtures {
            let file_path = directory_path.join(&data_file_fixture.files);
            generate_random_tsv(&file_path, data_file_fixture.size, data_file_fixture.gzip)?;
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct DirectoryConfig {
        directories: HashMap<String, Vec<DataFileFixture>>,
}

pub struct TestEnvironment {
    temp_dir: TempDir,
    main_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct DataFileFixture {
    files: String,
    size: usize, // size in bytes
    gzip: bool,
}

impl TestEnvironment {
    // Create a new TestEnvironment
    pub fn new() -> Result<Self, std::io::Error> {
        let pwd = env::current_dir()?;
        let temp_dir = TempDir::new()?;

        // Change the current working directory to the temporary directory
        env::set_current_dir(&temp_dir)?;

        info!("temp_dir: {:?}", temp_dir);

        Ok(Self {
            temp_dir,
            main_dir: pwd,
        })
    }

    pub fn cd_temp(&self) -> Result<(), std::io::Error> {
        env::set_current_dir(&self.temp_dir)
    }

    pub fn cd_tests(&self) -> Result<(), std::io::Error> {
        env::set_current_dir(&self.main_dir)
    }

    pub fn build_project_directories(&self, yaml_config: &str) -> Result<()> {
        let config: DirectoryConfig = serde_yaml::from_str(&yaml_config).unwrap();
        info!("config: {:?}", config);
        let base_path = self.temp_dir.path();
        generate_directory_structure(&config, &base_path);
        Ok(())
    }

    // Method to get the path of the temporary directory
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    pub fn change_directory(&self, sub_dir_name: &str) {
        let sub_dir_path = self.temp_dir.path().join(sub_dir_name);

        // Create the subdirectory if it does not exist
        std::fs::create_dir_all(&sub_dir_path);

        // Change the current working directory to the subdirectory
        env::set_current_dir(&sub_dir_path);
    }

}

impl Drop for TestEnvironment {
    // Revert to the original working directory and clean up the temporary directory
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.main_dir);
        // TempDir is automatically deleted when it's dropped
    }
}

pub fn read_keep_temp() -> bool {
    return env::var("KEEP_TEMP_DIR").is_ok()
}


