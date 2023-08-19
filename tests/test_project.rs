#[allow(unused_imports)]
use log::{info, trace, debug};
use std::path::{Path,PathBuf};

mod common;
use common::{setup,TestFixture};


#[cfg(test)]
mod tests {
    use log::info;
    use super::setup;
    use std::path::PathBuf;

    #[test]
    fn test_fixture() {
        let fixture = setup();
        // test that the fixtures were created
        //info!("files: {:?}", test_env.files);
        if let Some(fixture_files) = &fixture.env.files {
            for file in fixture_files {
                assert!(file.exists());
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
    async fn test_status() {
        let mut fixture = setup();
        let path_context = fixture.project.path_context();
        let statuses = fixture.project.data
            .status(&path_context, false).await
            .expect("Error in getting statuses.");
        assert!(statuses.is_empty());

        //let data_file = fixture.env.files.unwrap().first().unwrap();
        let files = fixture.env.files.as_ref().unwrap();
        let add_files: Vec<String> = files.first()
            .and_then(|f| f.to_str()) // Convert PathBuf to Option<&str>
            .map(|s| s.to_string())   // Convert &str to String
            .into_iter().collect();   // Convert Option<String> to Vec<String>
        info!("added files: {:?}", add_files);
        let _ = fixture.project.add(&add_files);

        let statuses = fixture.project.data
            .status(&path_context, false).await
            .expect("Error in getting statuses.");

        assert!(statuses.len() == 1);
        //assert!(statuses.get(key));
    }


}

