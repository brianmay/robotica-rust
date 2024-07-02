use robotica_backend::{
    pipes::stateless,
    services::{
        persistent_state::{self, PersistentStateRow},
        tesla::api::{self, Token},
    },
    spawn,
};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{error, info};

use crate::InitState;

#[derive(Debug)]
struct Meters {
    api: api::Meters,
}

impl Meters {
    fn new() -> Self {
        Self {
            api: api::Meters::new(),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to get token: {0}")]
    PersistentStateError(#[from] persistent_state::Error),
}

pub fn run(state: &InitState) -> Result<stateless::Receiver<Arc<Token>>, Error> {
    // let tesla_id = tesla.tesla_id;
    let (tx, rx) = stateless::create_pipe("tesla_token");

    let tesla_secret = state.persistent_state_database.for_name("tesla_token");
    let mut token = Token::get(&tesla_secret)?;
    let meters = Meters::new();

    spawn(async move {
        let mut refresh_token_timer = tokio::time::interval(Duration::from_secs(3600));

        check_token(&mut token, &tesla_secret, &meters).await;
        test_tesla_api(&token, &meters).await;
        tx.try_send(Arc::new(token.clone()));

        loop {
            refresh_token_timer.tick().await;
            check_token(&mut token, &tesla_secret, &meters).await;
            tx.try_send(Arc::new(token.clone()));
        }
    });

    Ok(rx)
}

async fn check_token(
    token: &mut Token,
    tesla_secret: &PersistentStateRow<Token>,
    counters: &Meters,
) {
    info!("Refreshing state, token expiration: {:?}", token.expires_at);
    token
        .check(tesla_secret, &counters.api)
        .await
        .unwrap_or_else(|err| {
            error!("Failed to refresh token: {}", err);
        });
    info!("Token expiration: {:?}", token.expires_at);
}

async fn test_tesla_api(token: &Token, counters: &Meters) {
    let _data = match token.get_products(&counters.api).await {
        Ok(data) => data,
        Err(err) => {
            error!("Failed to get vehicles: {}", err);
            return;
        }
    };
}
