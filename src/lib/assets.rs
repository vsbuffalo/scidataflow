use url::Url;

#[derive(Debug)]
pub struct GitHubRepo {
    username: String,
    repository: String,
}

impl GitHubRepo {
    /// Create a new GitHubRepo from a URL string
    pub fn new(url_str: &str) -> Result<Self, String> {
        let parsed_url = Url::parse(url_str).map_err(|e| e.to_string())?;
        let path_segments: Vec<&str> = parsed_url.path_segments().ok_or("Invalid path".to_string())?.collect();

        if path_segments.len() < 2 {
            return Err("URL should contain both username and repository".to_string());
        }

        Ok(Self {
            username: path_segments[0].to_string(),
            repository: path_segments[1].to_string(),
        })
    }

    /// Create the URL to download a file from the GitHub repository.
    pub fn url(&self, file_path: &str) -> String {
        format!(
            "https://github.com/{}/{}/raw/main/{}",
            self.username, self.repository, file_path
        )
    }
}
