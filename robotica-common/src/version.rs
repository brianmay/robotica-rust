//! The version information for this build

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// The date that this build was created
pub const BUILD_DATE: Option<&str> = option_env!("BUILD_DATE");

/// The git commit hash that this build was created from
pub const VCS_REF: Option<&str> = option_env!("VCS_REF");

/// The version of this build
#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Build date: {}\nVCS ref: {}",
            self.build_date, self.vcs_ref
        )
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used)]

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
