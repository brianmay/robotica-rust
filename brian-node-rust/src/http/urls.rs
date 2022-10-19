use tracing::error;

use super::HttpState;

pub(crate) fn generate_url(state: &HttpState, url: &str) -> Result<String, url::ParseError> {
    Ok(state.root_url.join(url)?.into())
}

pub(crate) fn generate_url_or_default(state: &HttpState, url: &str) -> String {
    match generate_url(state, url) {
        Ok(url) => url,
        Err(err) => {
            error!("Failed to generate URL {}: {err:?}", url);
            url.to_string()
        }
    }
}
