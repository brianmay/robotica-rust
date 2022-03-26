use std::{net::IpAddr, str::FromStr};

use warp::Filter;

pub async fn start() {
    tokio::spawn(async {
        let hello = warp::path::end().map(|| "Hello! You were not invited. Go away.");

        let addr = IpAddr::from_str("::0").unwrap();
        warp::serve(hello).run((addr, 4000)).await;
    });
}
