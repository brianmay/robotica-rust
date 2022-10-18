use std::{env, net::IpAddr, str::FromStr};

use robotica_node_rust::spawn;
use warp::Filter;

pub async fn start() {
    spawn(async {
        let build_date = env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
        let vcs_ref = env::var("VCS_REF").unwrap_or_else(|_| "unknown".to_string());
        let hello_string = format!(
            "Hello from Robotica! Build date: {}, VCS ref: {}",
            build_date, vcs_ref
        );

        let hello = warp::path::end().map(move || hello_string.clone());

        let addr = IpAddr::from_str("::0").unwrap();
        warp::serve(hello).run((addr, 4000)).await;
    });
}
