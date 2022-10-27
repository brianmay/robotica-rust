//! The version information for this build

/// The date that this build was created
pub const BUILD_DATE: Option<&str> = option_env!("BUILD_DATE");

/// The git commit hash that this build was created from
pub const VCS_REF: Option<&str> = option_env!("VCS_REF");
