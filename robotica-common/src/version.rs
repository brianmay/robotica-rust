//! The version information for this build

use std::fmt::{Display, Formatter};

use crate::{protobuf::ProtobufIntoFrom, protos};

/// The date that this build was created
pub const BUILD_DATE: Option<&str> = option_env!("BUILD_DATE");

/// The git commit hash that this build was created from
pub const VCS_REF: Option<&str> = option_env!("VCS_REF");

/// The version of this build
#[derive(Debug, Clone)]
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

impl ProtobufIntoFrom for Version {
    type Protobuf = protos::Version;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            build_date: self.build_date,
            vcs_ref: self.vcs_ref,
        }
    }

    fn from_protobuf(version: Self::Protobuf) -> Option<Self> {
        Some(Self {
            build_date: version.build_date,
            vcs_ref: version.vcs_ref,
        })
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
