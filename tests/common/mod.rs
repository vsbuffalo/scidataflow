///! Testing Utility Functions

#[allow(unused_imports)]
use anyhow::{anyhow, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use lazy_static::lazy_static;
use log::info;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use serde_derive::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;
use tempfile::TempDir;

use scidataflow::lib::data::StatusEntry;
use scidataflow::lib::project::Project;

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

fn create_seeded_rng() -> StdRng {
    let seed = 0;
    StdRng::seed_from_u64(seed)
}

pub fn generate_random_tsv(
    file_path: &Path,
    size: usize,
    gzip: bool,
    rng: &mut StdRng,
) -> Result<()> {
    let file = File::create(file_path)?;
    let writer: Box<dyn Write> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    let mut writer = BufWriter::new(writer);

    let mut bytes_written = 0;

    while bytes_written < size {
        let value: u32 = rng.gen();
        let line = format!("{}\t{}\t{}\t{}\n", value, value, value, value);
        bytes_written += line.len();
        writer.write_all(line.as_bytes())?;
    }
    writer.flush()?;
    Ok(())
}

fn generate_directory_structure(
    data_fixtures: &Vec<DataFileFixture>,
    base_path: &Path,
    cache_dir: &Path,
    rng: &mut StdRng,
) -> Result<()> {
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
            generate_random_tsv(&file_path, size_in_bytes, is_gzip, rng)?;
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
    pub files: Option<Vec<DataFileFixture>>,
    pub rng: StdRng,
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

        let rng = create_seeded_rng();
        Ok(Self {
            name: name.to_string(),
            temp_dir,
            main_dir: pwd,
            cache_dir,
            files: None,
            rng,
        })
    }

    pub fn build_project_directories(&mut self, data_fixtures: Vec<DataFileFixture>) -> Result<()> {
        generate_directory_structure(
            &data_fixtures,
            &self.temp_dir.path(),
            &self.cache_dir,
            &mut self.rng,
        )?;
        self.files = Some(data_fixtures);
        Ok(())
    }

    pub fn get_file_path(&self, file_path: &str) -> PathBuf {
        self.temp_dir.path().join(PathBuf::from(file_path))
    }
}

#[allow(dead_code)] // will implement later
pub fn read_keep_temp() -> bool {
    return env::var("KEEP_TEMP_DIR").is_ok();
}

impl Drop for TestEnvironment {
    // Revert to the original working directory and clean up the temporary directory
    fn drop(&mut self) {
        let _ = env::set_current_dir(&self.main_dir);
    }
}

pub async fn setup(do_add: bool) -> TestFixture {
    lazy_static! {
        static ref INIT_LOGGING: Once = Once::new();
    }

    INIT_LOGGING.call_once(|| {
        env_logger::init();
    });

    let project_name = "test_project".to_string();
    let mut test_env =
        TestEnvironment::new(&project_name).expect("Error creating test environment.");
    let data_fixtures = make_mock_fixtures();
    let _ = test_env.build_project_directories(data_fixtures);

    // initializes sciflow in the test environment
    let current_dir = env::current_dir().unwrap();
    info!(
        "temp_dir: {:?}, current directory: {:?}",
        test_env.temp_dir, current_dir
    );
    let _ = Project::set_config(
        &Some("Joan B. Scientist".to_string()),
        &Some("joan@ucberkely.edu".to_string()),
        &Some("UC Berkeley".to_string()),
    );
    let _ = Project::init(Some(project_name));
    let mut project = Project::new().expect("setting up TestFixture failed");

    if do_add {
        // add the files that should be added (e.g. a setup further
        // in setting up the mock project)
        // get the files to add
        let files = &test_env.files.as_ref().unwrap();
        let add_files: Vec<String> = files
            .into_iter()
            .filter(|f| f.add)
            .map(|f| f.path.clone())
            .collect();

        // add those files
        let _ = project.add(&add_files).await;
    }

    TestFixture {
        env: test_env,
        project,
    }
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

pub async fn get_statuses(
    fixture: &mut TestFixture,
    path_context: &Path,
) -> Vec<(PathBuf, StatusEntry)> {
    let statuses = fixture
        .project
        .data
        .status(&path_context, false)
        .await
        .expect("Error in getting statuses.");
    iter_status_entries(&statuses)
        .map(|(path, status)| (path, status.clone()))
        .collect()
}

pub async fn get_statuses_map(
    fixture: &mut TestFixture,
    path_context: &Path,
) -> HashMap<PathBuf, StatusEntry> {
    let statuses = fixture
        .project
        .data
        .status(&path_context, false)
        .await
        .expect("Error in getting statuses.");
    iter_status_entries(&statuses)
        .map(|(path, status)| (path, status.clone()))
        .collect()
}
