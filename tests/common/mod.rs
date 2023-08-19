///! Testing Utility Functions

#[allow(unused_imports)]
use anyhow::{anyhow,Result};
use log::info;
use std::env;
use rand::Rng;
use std::path::{Path,PathBuf};
use std::fs::File;
use std::io::Write;
use std::collections::{BTreeMap};
use tempfile::TempDir;
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::io::BufWriter;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::create_dir_all;
use std::sync::Once;
use lazy_static::lazy_static;

use sciflow::lib::project::Project;
use sciflow::lib::data::StatusEntry;

pub fn make_mock_fixtures() -> Vec<DataFileFixture> { 
    let files = vec![
        DataFileFixture {
            path: "data/data.tsv".to_string(),
            size: 5,
            add: true,
            track: false,
        },
        DataFileFixture {
            path: "data/supplement/big_1.tsv.gz".to_string(),
            size: 50,
            add: true,
            track: true,
        },
        DataFileFixture {
            path: "data/supplement/big_2.tsv.gz".to_string(),
            size: 10,
            add: true,
            track: true,
        },
        DataFileFixture {
            path: "data/raw/medium.tsv.gz".to_string(),
            size: 10,
            add: true,
            track: true,
        },
        ];
    files
}


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


fn generate_directory_structure(data_fixtures: &Vec<DataFileFixture>, base_path: &Path, cache_dir: &Path) -> Result<()> {
    for data_file_fixture in data_fixtures {
        let file_path = base_path.join(&data_file_fixture.path);
        let directory_path = file_path.parent().unwrap();
        create_dir_all(directory_path)?;

        let cached_file_path = cache_dir.join(&data_file_fixture.path);
        let cached_directory_path = cached_file_path.parent().unwrap();
        create_dir_all(cached_directory_path)?; // Ensure the directory exists in the cache

        if cached_file_path.exists() {
            std::fs::copy(&cached_file_path, &file_path)?;
        } else {
            let is_gzip = file_path.extension().map_or(false, |ext| ext == "gz");
            let size_in_bytes = data_file_fixture.size * 1_000_000;
            generate_random_tsv(&file_path, size_in_bytes, is_gzip)?;
            std::fs::copy(&file_path, &cached_file_path)?; // Now this should work
        }
    }
    Ok(())
}

pub struct TestEnvironment {
    pub name: String,
    pub temp_dir: TempDir,
    pub main_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub files: Option<Vec<DataFileFixture>>
}

pub struct TestFixture {
    pub env: TestEnvironment,
    pub project: Project,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataFileFixture {
    pub path: String,
    pub size: usize, // size in megabytes
    pub add: bool,
    pub track: bool,
}


impl TestEnvironment {
    // Create a new TestEnvironment
    pub fn new(name: &str) -> Result<Self> {
        let pwd = env::current_dir()?;
        let temp_dir = TempDir::new()?;
        let cache_dir = pwd.join(format!("tests/test_data/cached/{}/", name));
        create_dir_all(&cache_dir)?;

        // Change the current working directory to the temporary directory
        env::set_current_dir(&temp_dir)?;

        info!("temp_dir: {:?}", temp_dir);

        Ok(Self {
            name: name.to_string(),
            temp_dir,
            main_dir: pwd,
            cache_dir,
            files: None,
        })
    }

    pub fn build_project_directories(&mut self, data_fixtures: Vec<DataFileFixture>) -> Result<()> {
        generate_directory_structure(&data_fixtures, &self.temp_dir.path(), &self.cache_dir)?;
        self.files = Some(data_fixtures);
        Ok(())
    }

    pub fn get_file_path(&self, file_path: &str) -> PathBuf {
        self.temp_dir.path().join(PathBuf::from(file_path))
    }

}

pub fn read_keep_temp() -> bool {
    return env::var("KEEP_TEMP_DIR").is_ok()
}

impl Drop for TestEnvironment {
    // Revert to the original working directory and clean up the temporary directory
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.main_dir);
    }
}

pub fn setup() -> TestFixture {
    lazy_static! {
        static ref INIT_LOGGING: Once = Once::new();
    }

    INIT_LOGGING.call_once(|| {
        env_logger::init();
    });

    let project_name = "test_project".to_string();
    let mut test_env = TestEnvironment::new(&project_name).expect("Error creating test environment.");
    let data_fixtures = make_mock_fixtures();
    let _ = test_env.build_project_directories(data_fixtures);

    // initializes sciflow in the test environment
    let _ = Project::init(Some(project_name));
    let project = Project::new().expect("setting up TestFixture failed");

    TestFixture { env: test_env, project }
}


pub fn iter_status_entries<'a>(
    statuses: &'a BTreeMap<String, Vec<StatusEntry>>,
) -> impl Iterator<Item = (PathBuf, &'a StatusEntry)> + 'a {
    statuses.iter().flat_map(|(dir, entries)| {
        entries.iter().map(move |status| {
            let mut path = PathBuf::from(dir);
            path.push(&status.name);
            (path, status)
        })
    })
}


