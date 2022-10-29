use openid::{error::ClientError, DiscoveredClient, Options, Token, Userinfo};
use thiserror::Error;
use tracing::info;

#[derive(Debug, Clone)]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

pub struct Client {
    oidc_client: openid::Client,
    config: Config,
}

#[derive(Error, Debug)]
pub enum Error {
    // Parse error
    #[error("Parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    // OIDC error
    #[error("OpenID error: {0}")]
    OpenId(#[from] openid::error::Error),

    // OIDC error
    #[error("OpenID Client error: {0}")]
    OpenIdClient(#[from] ClientError),

    // No Token error
    #[error("No token")]
    NoToken,
}

impl Client {
    pub async fn new(config: Config) -> Result<Client, Error> {
        let cloned_config = config.clone();

        let client_id = config.client_id;
        let client_secret = config.client_secret;
        let redirect = Some(config.redirect_uri);
        let issuer = reqwest::Url::parse(&config.issuer)?;

        let client = DiscoveredClient::discover(client_id, client_secret, redirect, issuer).await?;

        let client = Client {
            oidc_client: client,
            config: cloned_config,
        };

        Ok(client)
    }

    pub fn get_auth_url(&self, origin_url: &str) -> String {
        let scopes = self.config.scopes.join(" ");

        let auth_url = self.oidc_client.auth_url(&Options {
            scope: Some(scopes),
            state: Some(origin_url.to_string()),
            ..Default::default()
        });

        auth_url.into()
    }

    pub async fn request_token(&self, code: &str) -> Result<(Token, Userinfo), Error> {
        let mut token: Token = self.oidc_client.request_token(code).await?.into();

        if let Some(id_token) = token.id_token.as_mut() {
            self.oidc_client.decode_token(id_token)?;
            self.oidc_client.validate_token(id_token, None, None)?;
            info!("token: {:?}", id_token);
        } else {
            return Err(Error::NoToken);
        }

        let user_info = self.oidc_client.request_userinfo(&token).await?;

        info!("user info: {:?}", user_info);

        Ok((token, user_info))
    }
}
