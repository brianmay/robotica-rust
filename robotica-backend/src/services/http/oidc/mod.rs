mod claims;

use openid::{error::ClientError, Discovered, Options};
use robotica_common::user::User;
use thiserror::Error;
use tracing::debug;

use super::errors::ResponseError;

#[derive(Debug, Clone)]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: String,
}

type OpenIdClient = openid::Client<Discovered, claims::StandardClaims>;
type Token = openid::Token<claims::StandardClaims>;

pub struct Client {
    oidc_client: OpenIdClient,
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
    pub async fn new(config: &Config) -> Result<Client, Error> {
        let cloned_config = config.clone();

        let client_id = config.client_id.clone();
        let client_secret = config.client_secret.clone();
        let redirect = Some(config.redirect_uri.clone());
        let issuer = reqwest::Url::parse(&config.issuer)?;

        let client = OpenIdClient::discover(client_id, client_secret, redirect, issuer).await?;

        let client = Client {
            oidc_client: client,
            config: cloned_config,
        };

        Ok(client)
    }

    pub async fn renew(&self) -> Result<Client, Error> {
        Self::new(&self.config).await
    }

    pub fn get_auth_url(&self, origin_url: &str) -> String {
        let auth_url = self.oidc_client.auth_url(&Options {
            scope: Some(self.config.scopes.to_string()),
            state: Some(origin_url.to_string()),
            ..Default::default()
        });

        auth_url.into()
    }

    pub async fn login(&self, code: &str) -> Result<User, ResponseError> {
        let mut token: Token = self
            .oidc_client
            .request_token(code)
            .await
            .map_err(|err| ResponseError::bad_request(format!("Request token failed: {err}")))?
            .into();

        if let Some(id_token) = token.id_token.as_mut() {
            debug!("token: {:?}", id_token);
            self.oidc_client
                .decode_token(id_token)
                .map_err(|err| ResponseError::bad_request(format!("Token decode failed: {err}")))?;
            self.oidc_client
                .validate_token(id_token, None, None)
                .map_err(|err| {
                    ResponseError::bad_request(format!("Token validation failed: {err}"))
                })?;
            debug!("token: {:?}", id_token);
        } else {
            return Err(ResponseError::bad_request("No id token"));
        }

        let no_groups = vec![];
        let groups = token
            .id_token
            .as_ref()
            .and_then(|id_token| id_token.payload().ok())
            .map_or(&no_groups, |claims| &claims.groups);

        let user_info = self
            .oidc_client
            .request_userinfo(&token)
            .await
            .map_err(|err| ResponseError::bad_request(format!("Request userinfo failed: {err}")))?;

        debug!("groups: {:?}", groups);
        debug!("user info: {:?}", user_info);

        let sub = user_info
            .sub
            .ok_or_else(|| ResponseError::internal_error("No sub in user info"))?;

        let name = user_info
            .name
            .ok_or_else(|| ResponseError::internal_error("No name in user info"))?;

        let email = user_info
            .email
            .ok_or_else(|| ResponseError::internal_error("No email in user info"))?;

        let is_admin = groups.contains(&"admin".to_string());

        Ok(User {
            sub,
            name,
            email,
            is_admin,
        })
    }
}
