use axum::{extract::State, Json};
use robotica_common::config::Config as UiConfig;
use robotica_common::robotica::http_api::ApiResponse;
use robotica_common::user::User;
use std::sync::Arc;
use tap::Pipe;
use tower_sessions::Session;

use super::super::get_user;
use super::errors::ResponseError;

/// Get the UI configuration for the logged in user.
///
/// Returns the default profile filtered to the views the user's groups
/// allow. This is what the web frontend renders: users only see the views
/// (and therefore the navigation items) they have access to.
///
/// # Errors
///
/// Returns [`ResponseError::AuthenticationFailed`] if not logged in, or
/// [`ResponseError::InternalError`] if no ui configuration is available.
pub async fn get_ui_config(
    State(ui_config): State<Option<Arc<UiConfig>>>,
    session: Session,
) -> Result<Json<ApiResponse<UiConfig>>, ResponseError> {
    let Some(user) = get_user(&session).await else {
        return Err(ResponseError::AuthenticationFailed);
    };

    let Some(config) = ui_config.as_ref() else {
        return Err(ResponseError::NotFoundError());
    };

    let config = filter_for_user(config, &user);

    config.pipe(ApiResponse::success).pipe(Json).pipe(Ok)
}

/// Filter the config to the views the user's groups allow.
fn filter_for_user(config: &UiConfig, user: &User) -> UiConfig {
    let groups: Vec<String> = user.groups.iter().map(|g| g.name.clone()).collect();
    config.filter_allowed(&groups)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use robotica_common::config::ViewKind;
    use robotica_common::user::Group;

    fn user(group_names: &[&str]) -> User {
        User {
            id: 1,
            oidc_id: "oidc".to_string(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            is_admin: false,
            groups: group_names
                .iter()
                .zip(0i32..)
                .map(|(name, id)| Group {
                    id,
                    name: (*name).to_string(),
                })
                .collect(),
        }
    }

    fn config() -> UiConfig {
        let mut restricted = UiConfig {
            instance: "home".to_string(),
            views: vec![],
        };
        restricted.views.push(robotica_common::config::ViewConfig {
            id: "family".to_string(),
            title: "Family".to_string(),
            menu: None,
            groups: vec!["family".to_string()],
            kind: ViewKind::Tags,
        });
        restricted.views.push(robotica_common::config::ViewConfig {
            id: "everyone".to_string(),
            title: "Everyone".to_string(),
            menu: None,
            groups: vec![],
            kind: ViewKind::Schedule,
        });
        restricted
    }

    #[test]
    fn test_filter_for_user() {
        let config = config();

        let guest = filter_for_user(&config, &user(&[]));
        let ids: Vec<&str> = guest.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["everyone"]);

        let family = filter_for_user(&config, &user(&["family"]));
        let ids: Vec<&str> = family.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["family", "everyone"]);
    }
}
