//! The version information for this build

use serde::Serialize;

/// The date that this build was created
pub const BUILD_DATE: Option<&str> = option_env!("BUILD_DATE");

/// The git commit hash that this build was created from
pub const VCS_REF: Option<&str> = option_env!("VCS_REF");

/// The version of this build
#[derive(Debug, Serialize)]
pub struct Version {
    /// The date that this build was created
    pub build_date: String,

    /// The git commit hash that this build was created from
    pub vcs_ref: String,
}

impl Version {
    /// Get the version information for this build
    #[must_use]
    pub fn get() -> Version {
        Version {
            build_date: BUILD_DATE.unwrap_or("unknown").into(),
            vcs_ref: VCS_REF.unwrap_or("unknown").into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_version() {
        let version = Version {
            build_date: "2021-01-01".into(),
            vcs_ref: "123456".into(),
        };
        let string = serde_json::to_string(&version).unwrap();
        assert_eq!(string, r#"{"build_date":"2021-01-01","vcs_ref":"123456"}"#);
    }
}
