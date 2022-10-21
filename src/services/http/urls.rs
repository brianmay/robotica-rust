use tracing::error;

pub fn generate_url(root_url: &reqwest::Url, path: &str) -> Result<String, url::ParseError> {
    Ok(root_url.join(path)?.into())
}

pub fn generate_url_or_default(root_url: &reqwest::Url, path: &str) -> String {
    match generate_url(root_url, path) {
        Ok(url) => url,
        Err(err) => {
            error!("Failed to generate URL {}: {err:?}", path);
            path.to_string()
        }
    }
}
