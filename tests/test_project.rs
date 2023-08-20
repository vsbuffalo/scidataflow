#[allow(unused_imports)]
use log::{info, trace, debug};

mod common;
use common::{setup,get_statuses,generate_random_tsv};


#[cfg(test)]
mod tests {
    use log::info;
    use crate::common::get_statuses_map;

    use super::setup;
    use super::get_statuses;
    use super::generate_random_tsv;
    use std::path::PathBuf;
    use sciflow::lib::data::LocalStatusCode;

    #[test]
    fn test_fixture() {
        let fixture = setup(false);
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
        let fixture = setup(false);
        // test that init() creates the data manifest
        let data_manifest = fixture.env.get_file_path("data_manifest.yml");
        info!("Checking for file at path: {}", data_manifest.display()); // Add this log
        assert!(data_manifest.exists(), "Project::init() did not create 'data_manifest.yml'");
    }

    #[tokio::test]
    async fn test_add_status_current() {
        let mut fixture = setup(false);
        let path_context = fixture.project.path_context();
        let statuses = get_statuses(&mut fixture, &path_context).await;

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

        // get statuses again
        let statuses = get_statuses(&mut fixture, &path_context).await;

        info!("statuses: {:?}", statuses);

        for (full_path, status) in statuses {
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

    #[tokio::test]
    async fn test_add_status_modified() {
        let mut fixture = setup(true);
        let path_context = fixture.project.path_context();
        let statuses = get_statuses_map(&mut fixture, &path_context).await;

        // Check initial status is Current
        let file_to_check = PathBuf::from("data/data.tsv");
        let initial_status_option = statuses.get(&file_to_check).unwrap().local_status.clone();
        let initial_status = initial_status_option.unwrap().clone();
        assert_eq!(initial_status, LocalStatusCode::Current);

        // Modifying the file
        let _ = generate_random_tsv(&file_to_check.clone(), 5, false, &mut fixture.env.rng);

        // Now, let's check the status is modified.
        let updated_statuses = get_statuses_map(&mut fixture, &path_context).await;
        let updated_status_option = updated_statuses.get(&file_to_check).unwrap().local_status.clone();
        let updated_status = updated_status_option.unwrap().clone();
        assert_eq!(updated_status, LocalStatusCode::Modified);

        // Now, let's re-add the file and make sure the status goes back to current.
        let re_add_files = vec![file_to_check.to_string_lossy().to_string()];
        let _ = fixture.project.add(&re_add_files);

        let readd_statuses = get_statuses_map(&mut fixture, &path_context).await;
        let readd_status_option = readd_statuses.get(&file_to_check).unwrap().local_status.clone();
        let readd_status = readd_status_option.unwrap().clone();
        assert_eq!(readd_status, LocalStatusCode::Current);
    }

}

