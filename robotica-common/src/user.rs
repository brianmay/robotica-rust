//! Struct for end user
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[cfg(feature = "websockets")]
use crate::{protobuf::ProtobufIntoFrom, protos};

/// An authenticated end user
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Group {
    /// The group's identifier
    pub id: i32,

    /// The name of the group
    pub name: String,
}

/// An authenticated end user
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    /// The user's identifier
    pub id: i32,

    /// The user's identifier
    pub oidc_id: String,

    /// The name of the user
    pub name: String,

    /// The email of the user
    pub email: String,

    /// Is the user an admin?
    pub is_admin: bool,

    /// The user's groups
    pub groups: Vec<Group>,
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for Group {
    type Protobuf = protos::Group;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            id: self.id,
            name: self.name,
        }
    }

    fn from_protobuf(group: Self::Protobuf) -> Option<Self> {
        Some(Self {
            id: group.id,
            name: group.name,
        })
    }
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for User {
    type Protobuf = protos::User;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            id: self.id,
            oidc_id: self.oidc_id,
            name: self.name,
            email: self.email,
            is_admin: self.is_admin,
            groups: self
                .groups
                .into_iter()
                .map(super::protobuf::ProtobufIntoFrom::into_protobuf)
                .collect(),
        }
    }

    fn from_protobuf(user: Self::Protobuf) -> Option<Self> {
        Some(Self {
            id: user.id,
            oidc_id: user.oidc_id,
            name: user.name,
            email: user.email,
            is_admin: user.is_admin,
            groups: user
                .groups
                .into_iter()
                .filter_map(super::protobuf::ProtobufIntoFrom::from_protobuf)
                .collect(),
        })
    }
}
