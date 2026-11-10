//! UI configuration: the views and profiles published to MQTT.
//!
//! The backend owns the UI configuration. Views are declared once in the
//! `ui` section of the backend configuration; profiles then reference
//! views by id. Each profile is published retained to
//! `robotica/config/{name}`, where clients pick it up:
//!
//! - the `default` profile is consumed by the web frontend;
//! - per device profiles (e.g. `brian`) are consumed by the wall panel
//!   whose `ui_config_name` matches, and contain just the views for that
//!   device.

use std::collections::BTreeMap;

use robotica_common::config::{Config as CommonConfig, ViewConfig};
use robotica_tokio::services::mqtt::MqttTx;
use serde::Deserialize;
use thiserror::Error;
use tracing::{info, warn};

use robotica_common::mqtt::{Json, QoS, Retain};

/// An error in the UI configuration.
#[derive(Error, Debug)]
pub enum Error {
    /// Two views have the same id.
    #[error("Duplicate view id: {0}")]
    DuplicateViewId(String),

    /// A profile references a view id that does not exist.
    #[error("Profile {profile} references unknown view id: {id}")]
    UnknownViewId {
        /// The name of the profile.
        profile: String,
        /// The unknown view id.
        id: String,
    },

    /// A profile references the same view twice.
    #[error("Profile {profile} references view id {id} more than once")]
    DuplicateViewInProfile {
        /// The name of the profile.
        profile: String,
        /// The duplicated view id.
        id: String,
    },

    /// A profile that does not exist was requested.
    #[error("Unknown profile: {0}")]
    UnknownProfile(String),
}

/// UI configuration: the views and the profiles to publish.
#[derive(Deserialize, Clone)]
pub struct UiConfig {
    /// The name of the server in MQTT topics.
    pub instance: String,

    /// The views, declared once.
    pub views: Vec<ViewConfig>,

    /// The named profiles to publish; each profile is a list of view ids.
    pub profiles: BTreeMap<String, Vec<String>>,
}

impl UiConfig {
    /// Validate the configuration.
    ///
    /// Checks that view ids are unique and that every profile references
    /// known view ids exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the configuration is invalid.
    pub fn validate(&self) -> Result<(), Error> {
        let mut seen: Vec<&str> = Vec::with_capacity(self.views.len());
        for view in &self.views {
            if seen.contains(&view.id.as_str()) {
                return Err(Error::DuplicateViewId(view.id.clone()));
            }
            seen.push(&view.id);
        }

        for (profile, view_ids) in &self.profiles {
            for id in view_ids {
                if !seen.contains(&id.as_str()) {
                    return Err(Error::UnknownViewId {
                        profile: profile.clone(),
                        id: id.clone(),
                    });
                }
                if view_ids.iter().filter(|other| *other == id).count() > 1 {
                    return Err(Error::DuplicateViewInProfile {
                        profile: profile.clone(),
                        id: id.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Build the UI configuration for a profile.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the profile references unknown or duplicate
    /// view ids.
    pub fn profile_config(&self, profile: &str) -> Result<CommonConfig, Error> {
        let Some(view_ids) = self.profiles.get(profile) else {
            return Err(Error::UnknownProfile(profile.to_string()));
        };

        let views: Vec<ViewConfig> = view_ids
            .iter()
            .map(|id| {
                self.views
                    .iter()
                    .find(|view| &view.id == id)
                    .cloned()
                    .ok_or_else(|| Error::UnknownViewId {
                        profile: profile.to_string(),
                        id: id.clone(),
                    })
            })
            .collect::<Result<_, _>>()?;

        Ok(CommonConfig {
            instance: self.instance.clone(),
            views,
        })
    }

    /// Publish every profile retained to `robotica/config/{name}`.
    ///
    /// The messages are queued on the MQTT client channel and published
    /// once the client connects.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if the configuration is invalid.
    pub fn publish_profiles(&self, mqtt: &MqttTx) -> Result<(), Error> {
        self.validate()?;

        if !self.profiles.contains_key("default") {
            warn!("No \"default\" ui profile configured; the web frontend will have no config");
        }

        for name in self.profiles.keys() {
            let config = self.profile_config(name)?;
            info!(
                "Publishing ui config profile {name} ({} views)",
                config.views.len()
            );
            mqtt.try_serialize_send(
                format!("robotica/config/{name}"),
                &Json(config),
                Retain::Retain,
                QoS::AtLeastOnce,
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use robotica_common::config::{ControlsConfig, ViewKind};

    fn view(id: &str) -> ViewConfig {
        ViewConfig {
            id: id.to_string(),
            title: id.to_string(),
            menu: None,
            groups: Vec::new(),
            kind: ViewKind::Controls(ControlsConfig { rows: vec![] }),
        }
    }

    fn ui_config(profiles: &[(&str, &[&str])]) -> UiConfig {
        UiConfig {
            instance: "home".to_string(),
            views: vec![view("brian"), view("jan"), view("tags"), view("schedule")],
            profiles: profiles
                .iter()
                .map(|(name, ids)| {
                    (
                        (*name).to_string(),
                        ids.iter().map(|id| (*id).to_string()).collect(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn test_validate_ok() {
        let config = ui_config(&[
            ("default", &["brian", "jan", "tags", "schedule"]),
            ("brian", &["brian", "tags", "schedule"]),
        ]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_unknown_view() {
        let config = ui_config(&[("default", &["brian", "missing"])]);
        match config.validate() {
            Err(Error::UnknownViewId { profile, id }) => {
                assert_eq!(profile, "default");
                assert_eq!(id, "missing");
            }
            other => panic!("expected UnknownViewId, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_duplicate_view() {
        let config = ui_config(&[("default", &["brian", "brian"])]);
        assert!(matches!(
            config.validate(),
            Err(Error::DuplicateViewInProfile { .. })
        ));
    }

    #[test]
    fn test_profile_config_resolves_views_in_order() {
        let config = ui_config(&[("brian", &["brian", "tags", "schedule"])]);
        let profile = config.profile_config("brian").unwrap();
        assert_eq!(profile.instance, "home");
        let ids: Vec<&str> = profile.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["brian", "tags", "schedule"]);
    }

    #[test]
    fn test_deserialize_ui_config_from_yaml() {
        let yaml = r"
instance: home
views:
  - id: brian
    title: Brian's Room
    menu: Bedrooms
    kind:
      type: controls
      rows: []
  - id: tags
    title: Tags
    kind:
      type: tags
profiles:
  default:
    - brian
    - tags
  brian:
    - brian
    - tags
";
        let config: UiConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.profile_config("brian").unwrap().views.len(), 2);
    }
}
