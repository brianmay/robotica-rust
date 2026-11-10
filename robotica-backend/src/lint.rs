//! Validate the backend configuration without running the backend.
//!
//! Loads the configuration exactly as the backend does at startup
//! (including merging the secrets file and environment overrides), then
//! validates the ui views and profiles and parses the executor's
//! referenced yaml files.
//!
//! Run as `robotica-backend lint [config-file]`; the nix module uses this
//! at build time so that a broken configuration fails the build instead
//! of the service at runtime.

use robotica_tokio::scheduling::{classifier, scheduler, sequencer};
use thiserror::Error;

use crate::config::Environment;

/// An error found while linting the configuration.
#[derive(Error, Debug)]
pub enum Error {
    /// The backend configuration is invalid.
    #[error(transparent)]
    Config(#[from] crate::config::Error),

    /// The ui configuration is invalid.
    #[error(transparent)]
    Ui(#[from] crate::ui::Error),

    /// The classifications file is invalid.
    #[error("classifications file: {0}")]
    Classifications(#[from] classifier::ConfigError),

    /// The schedule file is invalid.
    #[error("schedule file: {0}")]
    Schedule(#[from] scheduler::ConfigError),

    /// The sequences file is invalid.
    #[error("sequences file: {0}")]
    Sequences(#[from] sequencer::ConfigError),

    /// The schedule references a sequence that does not exist.
    #[error("schedule is inconsistent: {0}")]
    CheckSchedule(#[from] sequencer::ConfigCheckError),
}

/// Lint the configuration for the given environment.
///
/// Parses everything the backend would parse at startup, without
/// connecting to anything:
///
/// - the backend configuration itself (with the secrets file and
///   environment overrides merged in, exactly as at startup);
/// - the ui views and profiles;
/// - the executor's classifications, schedule and sequences files,
///   including a consistency check of the schedule against the sequences.
///
/// # Errors
///
/// Returns [`Error`] if any part of the configuration is invalid.
pub fn lint(environment: &Environment) -> Result<(), Error> {
    let config = environment.config()?;

    if let Some(ui_config) = &config.ui {
        ui_config.validate()?;
        for name in ui_config.profiles.keys() {
            ui_config.profile_config(name)?;
        }
    }

    if let Some(executor_config) = &config.executor {
        classifier::load_config(&executor_config.classifications_file)?;
        let schedule = scheduler::load_config(&executor_config.schedule_file)?;
        let sequences = sequencer::load_config(&executor_config.sequences_file)?;
        sequencer::check_schedule(&schedule, &sequences)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::path::PathBuf;

    /// Write a file into a per-test temp directory and return its path.
    fn write_file(test: &str, name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("robotica-backend-lint-tests")
            .join(test);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn environment(test: &str, config: &str) -> Environment {
        let config_file = write_file(test, "config.yaml", config);
        Environment {
            config_file,
            secrets_file: None,
            static_path: None,
            database_url: Some("postgresql://localhost/robotica".to_string()),
        }
    }

    fn executor_files(test: &str) -> String {
        let classifications = write_file(test, "classifications.yaml", "[]\n");
        let schedule = write_file(test, "schedule.yaml", "[]\n");
        let sequences = write_file(test, "sequences.yaml", "{}\n");
        format!(
            "executor:\n\
             \x20 instance: home\n\
             \x20 calendar_url: http://localhost/calendar\n\
             \x20 classifications_file: {classifications:?}\n\
             \x20 schedule_file: {schedule:?}\n\
             \x20 sequences_file: {sequences:?}\n"
        )
    }

    fn base_config(executor: &str, ui: &str) -> String {
        format!(
            "mqtt:\n  host: localhost\n  port: 1883\n\
             influxdb:\n  url: http://localhost:8086\n  token: token\n  database: sensors\n\
             persistent_state:\n  state_path: /tmp/robotica-lint-test\n\
             logging:\n  deployment_environment: test\n\
             cars: []\nlights: []\nstrips: []\nmetrics: []\nwater_heaters: []\n\
             hdmi_matrices: []\ndoor_monitors: []\npresence_trackers: []\n\
             occupancy_sensors: []\nnight_mode: []\nmessage_routes: []\nowntracks: []\n\
             {executor}\
             {ui}"
        )
    }

    #[test]
    fn test_lint_ok() {
        let environment = environment(
            "test_lint_ok",
            &base_config(
                &executor_files("test_lint_ok"),
                "ui:\n  instance: home\n  views: []\n  profiles:\n    default: []\n",
            ),
        );
        assert!(lint(&environment).is_ok(), "{:?}", lint(&environment));
    }

    #[test]
    fn test_lint_ui_error() {
        // The profile references a view id that does not exist.
        let environment = environment(
            "test_lint_ui_error",
            &base_config(
                &executor_files("test_lint_ui_error"),
                "ui:\n  instance: home\n  views: []\n  profiles:\n    default:\n      - missing\n",
            ),
        );
        match lint(&environment) {
            Err(Error::Ui(err)) => {
                assert!(err.to_string().contains("missing"), "{err}");
            }
            other => panic!("expected Ui error, got {other:?}"),
        }
    }

    #[test]
    fn test_lint_partial_config_with_dummy_secrets() {
        // Simulates the nix build time check: the generated config
        // provides only the non secret sections, and a dummy secrets
        // file (see `config_check_secrets` in the nix module) provides
        // the rest. The real secrets file is only decrypted at
        // activation time, so it is not available at build time.
        let test = "test_lint_partial_config_with_dummy_secrets";
        let secrets = write_file(
            test,
            "secrets.yaml",
            "database_url: postgresql://check:check@localhost/check\n\
             mqtt:\n  host: localhost\n  port: 1883\n\
             influxdb:\n  url: http://localhost:8086\n  token: check\n  database: check\n",
        );
        let config_file = write_file(
            test,
            "config.yaml",
            "persistent_state:\n  state_path: /tmp/robotica-lint-test\n\
                 logging:\n  deployment_environment: test\n\
                 cars: []\nlights: []\nstrips: []\nmetrics: []\nwater_heaters: []\n\
                 hdmi_matrices: []\ndoor_monitors: []\npresence_trackers: []\n\
                 occupancy_sensors: []\nnight_mode: []\nmessage_routes: []\nowntracks: []\n\
                 ui:\n  instance: home\n  views: []\n  profiles:\n    default: []\n",
        );
        let environment = Environment {
            config_file,
            secrets_file: Some(secrets),
            static_path: None,
            database_url: None,
        };
        assert!(lint(&environment).is_ok(), "{:?}", lint(&environment));
    }

    #[test]
    fn test_lint_bad_yaml() {
        let environment = environment("test_lint_bad_yaml", "this is not: [valid yaml\n");
        assert!(matches!(lint(&environment), Err(Error::Config(_))));
    }

    #[test]
    fn test_lint_missing_executor_file() {
        let environment = environment(
            "test_lint_missing_executor_file",
            &base_config(
                "executor:\n  instance: home\n  calendar_url: http://localhost/calendar\n  classifications_file: /nonexistent/classifications.yaml\n  schedule_file: /nonexistent/schedule.yaml\n  sequences_file: /nonexistent/sequences.yaml\n",
                "",
            ),
        );
        match lint(&environment) {
            Err(Error::Classifications(err)) => {
                assert!(err.to_string().contains("/nonexistent"), "{err}");
            }
            other => panic!("expected Classifications error, got {other:?}"),
        }
    }
}
