//! Influxdb stuff

use serde::Deserialize;
#[derive(Deserialize, Clone)]
pub struct Config {
    pub influxdb_url: String,
    pub influxdb_database: String,
}

impl Config {
    pub fn get_client(&self) -> influxdb::Client {
        influxdb::Client::new(&self.influxdb_url, &self.influxdb_database)
    }
}
