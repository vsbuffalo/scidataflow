#[allow(unused_imports)]
use log::{debug, info, trace};

mod common;
use common::{generate_random_tsv, get_statuses, setup};

#[cfg(test)]
mod tests {
    use crate::common::get_statuses_map;
    use log::info;

    use super::generate_random_tsv;
    use super::get_statuses;
    use super::setup;
    use scidataflow::lib::data::LocalStatusCode;
    use std::fs;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_fixture() {
        let fixture = setup(false).await;
        // test that the fixtures were created
        //info!("files: {:?}", test_env.files);
        if let Some(fixtures) = &fixture.env.files {
            for file in fixtures {
                assert!(PathBuf::from(&file.path).exists());
            }
        }
    }

    #[tokio::test]
    async fn test_init() {
        let fixture = setup(false).await;
        // test that init() creates the data manifest
        let data_manifest = fixture.env.get_file_path("data_manifest.yml");
        info!("Checking for file at path: {}", data_manifest.display()); // Add this log
        assert!(
            data_manifest.exists(),
            "Project::init() did not create 'data_manifest.yml'"
        );
    }

    #[tokio::test]
    async fn test_add_status_current() {
        let mut fixture = setup(false).await;
        let path_context = fixture.project.path_context();
        let statuses = get_statuses(&mut fixture, &path_context).await;

        // at this point the status should be empty
        assert!(statuses.is_empty());

        // get the files to add
        let files = &fixture.env.files.as_ref().unwrap();
        let add_files: Vec<String> = files
            .into_iter()
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
                assert!(
                    status.local_status.is_some(),
                    "File '{:?}' does not have a local status",
                    full_path
                );
                if let Some(local_status) = &status.local_status {
                    assert_eq!(
                        *local_status,
                        LocalStatusCode::Current,
                        "Added file '{:?}' does not have 'Current' status",
                        full_path
                    );
                }
            } else {
                // check that the status is None (not registered)
                assert!(
                    status.local_status.is_none(),
                    "File '{:?}' should not have a local status",
                    full_path
                );
            }
        }
    }

    #[tokio::test]
    async fn test_add_update_status_modified() {
        let mut fixture = setup(true).await;
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
        let updated_status_option = updated_statuses
            .get(&file_to_check)
            .unwrap()
            .local_status
            .clone();
        let updated_status = updated_status_option.unwrap().clone();
        assert_eq!(updated_status, LocalStatusCode::Modified);

        // Now, let's update these files
        let re_add_files = vec![file_to_check.to_string_lossy().to_string()];

        for file in &re_add_files {
            let files = vec![file.clone()];
            let result = fixture.project.update(Some(&files)).await;
            assert!(result.is_ok(), "re-adding raised Error!");
        }

        // and make sure the status goes back to current.
        let readd_statuses = get_statuses_map(&mut fixture, &path_context).await;
        let readd_status_option = readd_statuses
            .get(&file_to_check)
            .unwrap()
            .local_status
            .clone();
        let readd_status = readd_status_option.unwrap().clone();
        assert_eq!(readd_status, LocalStatusCode::Current);
    }

    #[tokio::test]
    async fn test_add_already_added_error() {
        let mut fixture = setup(true).await;

        if let Some(files) = &fixture.env.files {
            for file in files {
                let mut file_list = Vec::new();
                file_list.push(file.path.clone());
                let result = fixture.project.add(&file_list).await;

                // check that we get
                match result {
                    Ok(_) => assert!(false, "Expected an error, but got Ok"),
                    Err(err) => {
                        assert!(
                            err.to_string().contains("already registered"),
                            "Unexpected error: {:?}",
                            err
                        );
                    }
                };
            }
        }
    }

    #[tokio::test]
    async fn test_mv() {
        let mut fixture = setup(false).await;
        let path_context = fixture.project.path_context();
        let statuses = get_statuses(&mut fixture, &path_context).await;

        // at this point the status should be empty
        assert!(statuses.is_empty());

        // get the files to add
        let files = &fixture.env.files.as_ref().unwrap();
        let add_files: Vec<String> = files
            .into_iter()
            .filter(|f| f.add)
            .map(|f| f.path.clone())
            .collect();

        // add those files
        let _ = fixture.project.add(&add_files).await;

        let new_name = "data/data_alt.tsv";
        let target_path = PathBuf::from(new_name);

        let statuses = get_statuses(&mut fixture, &path_context).await;
        let exists = statuses.iter().any(|(path, _status)| path == &target_path);
        assert!(!exists); // not there before move

        // try moving a file (renaming)
        fixture.project.mv("data/data.tsv", new_name).await.unwrap();

        let exists = statuses.iter().any(|(path, _status)| path == &target_path);
        assert!(!exists); // now it should be there

        // now let's try moving to a directory
        fs::create_dir_all("new_data/").unwrap();
        fixture
            .project
            .mv("data/supplement/big_1.tsv.gz", "new_data/")
            .await
            .unwrap();

        let statuses = get_statuses(&mut fixture, &path_context).await;
        let target_path = PathBuf::from("data/supplement/big_1.tsv.gz");
        let exists = statuses.iter().any(|(path, _status)| path == &target_path);
        assert!(!exists); // now it should be there
    }
}
