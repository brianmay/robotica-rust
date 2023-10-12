//! Influxdb stuff

use serde::Deserialize;
#[derive(Deserialize, Clone)]
pub struct Config {
    pub url: String,
    pub database: String,
}

impl Config {
    pub fn get_client(&self) -> influxdb::Client {
        influxdb::Client::new(&self.url, &self.database)
    }
}
