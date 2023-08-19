#[allow(unused_imports)]
use log::{info, trace, debug};
use std::path::{Path,PathBuf};

mod common;
use common::{setup,iter_status_entries,TestFixture};

use sciflow::lib::data::LocalStatusCode;


#[cfg(test)]
mod tests {
    use log::info;
    use super::setup;
    use super::iter_status_entries;
    use std::path::PathBuf;
    use sciflow::lib::data::LocalStatusCode;

    #[test]
    fn test_fixture() {
        let fixture = setup();
        // test that the fixtures were created
        //info!("files: {:?}", test_env.files);
        if let Some(fixtures) = &fixture.env.files {
            for file in fixtures {
                assert!(PathBuf::from(&file.path).exists());
            }
        }
    }

    #[test]
    fn test_init() {
        let fixture = setup();
        // test that init() creates the data manifest
        let data_manifest = fixture.env.get_file_path("data_manifest.yml");
        info!("Checking for file at path: {}", data_manifest.display()); // Add this log
        assert!(data_manifest.exists(), "Project::init() did not create 'data_manifest.yml'");
    }

    #[tokio::test]
    async fn test_add_status_current() {
        let mut fixture = setup();
        let path_context = fixture.project.path_context();
        let statuses = fixture.project.data
            .status(&path_context, false).await
            .expect("Error in getting statuses.");

        // at this point the status should be empty
        assert!(statuses.is_empty());

        // get the files to add
        let files = &fixture.env.files.as_ref().unwrap();
        let add_files: Vec<String> = files.into_iter()
            .filter(|f| f.add)
            .map(|f| f.path.clone())
            .collect();

        // add those files
        let _ = fixture.project.add(&add_files);

        let statuses = fixture.project.data
            .status(&path_context, false).await
            .expect("Error in getting statuses.");

        info!("statuses: {:?}", statuses);

        for (full_path, status) in iter_status_entries(&statuses) {
            if add_files.contains(&full_path.to_string_lossy().to_string()) {
                // check that the status is current
                assert!(status.local_status.is_some(), "File '{:?}' does not have a local status", full_path);
                if let Some(local_status) = &status.local_status {
                    assert_eq!(*local_status, LocalStatusCode::Current, "Added file '{:?}' does not have 'Current' status", full_path);
                }
            } else {
                // check that the status is None (not registered)
                assert!(status.local_status.is_none(), "File '{:?}' should not have a local status", full_path);
            }
        }
    }
}

