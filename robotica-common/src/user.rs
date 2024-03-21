//! Struct for end user
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[cfg(feature = "websockets")]
use crate::{protobuf::ProtobufIntoFrom, protos};

/// An authenticated end user
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    /// The user's identifier
    pub sub: String,

    /// The name of the user
    pub name: String,

    /// The email of the user
    pub email: String,

    /// Is the user an admin?
    pub is_admin: bool,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for User {
    type Protobuf = protos::User;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            sub: self.sub,
            name: self.name,
            email: self.email,
            is_admin: self.is_admin,
        }
    }

    fn from_protobuf(user: Self::Protobuf) -> Option<Self> {
        Some(Self {
            sub: user.sub,
            name: user.name,
            email: user.email,
            is_admin: user.is_admin,
        })
    }
}
