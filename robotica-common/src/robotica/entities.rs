//! Entities represent a device in the system.
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// An identifier for a device in the system
pub trait AnyId {
    /// Convert the identifier to a string
    #[must_use]
    fn to_id_string(&self) -> String;

    /// Convert the identifier to components
    #[must_use]
    fn to_components(&self) -> Vec<&str>;

    /// Get the MQTT state topic for the entity
    #[must_use]
    fn get_state_topic(&self, name: &str) -> String {
        if name.is_empty() {
            format!("robotica/state/{}", self.to_id_string())
        } else {
            format!("robotica/state/{}/{name}", self.to_id_string())
        }
    }

    /// Get the MQTT command topic for the entity
    #[must_use]
    fn get_command_topic(&self, name: &str) -> String {
        if name.is_empty() {
            format!("robotica/command/{}", self.to_id_string())
        } else {
            format!("robotica/command/{}/{name}", self.to_id_string())
        }
    }
}

/// A unique identifier for a device
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Id {
    /// The device name
    pub device: String,
}

impl Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.device)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let device = String::deserialize(deserializer)?;
        Id::new(device).map_err(de::Error::custom)
    }
}

impl Id {
    /// Create a new identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidId`] if `device` contains characters other than
    /// alphanumeric characters or hyphens.
    pub fn new(device: impl Into<String>) -> Result<Self, Error> {
        let device = device.into();
        validate_string(&device)?;
        Ok(Self { device })
    }
}

impl AnyId for Id {
    fn to_id_string(&self) -> String {
        self.device.clone()
    }
    fn to_components(&self) -> Vec<&str> {
        vec![self.device.as_str()]
    }
}

/// A unique identifier for a device in a room
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct IdWithRoom {
    /// The room name
    pub room: String,
    /// The device name
    pub device: String,
}

impl Serialize for IdWithRoom {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_id_string())
    }
}

impl<'de> Deserialize<'de> for IdWithRoom {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let (room, device) = s.split_once('/').ok_or_else(|| {
            de::Error::custom(format!(
                "expected exactly one '/' separating room and device in {s:?}"
            ))
        })?;
        if device.contains('/') {
            return Err(de::Error::custom(format!(
                "expected exactly one '/' separating room and device in {s:?}"
            )));
        }
        if room.is_empty() || device.is_empty() {
            return Err(de::Error::custom(format!(
                "room and device must both be non-empty in {s:?}"
            )));
        }
        IdWithRoom::new(room, device).map_err(de::Error::custom)
    }
}

impl IdWithRoom {
    /// Create a new identifier with a room prefix.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidId`] if `room` or `device` contains characters
    /// other than alphanumeric characters or hyphens.
    pub fn new(room: impl Into<String>, device: impl Into<String>) -> Result<Self, Error> {
        let room = room.into();
        let device = device.into();
        validate_string(&room)?;
        validate_string(&device)?;
        Ok(Self { room, device })
    }
}

impl AnyId for IdWithRoom {
    fn to_id_string(&self) -> String {
        format!("{}/{}", self.room, self.device)
    }
    fn to_components(&self) -> Vec<&str> {
        vec![self.room.as_str(), self.device.as_str()]
    }
}

/// Errors that can occur when working with Ids
#[derive(Debug, Error)]
pub enum Error {
    /// The id contains invalid characters
    #[error("invalid id: {0}")]
    InvalidId(String),
}

fn validate_string(s: &str) -> Result<(), Error> {
    // does s contain only ASCII alphanumeric characters, hyphens, or underscores?
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidId(s.to_string()));
    }
    Ok(())
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_id_string())
    }
}

impl std::fmt::Display for IdWithRoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_id_string())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_id_new() {
        assert!(Id::new("device").is_ok());
        assert!(Id::new("device-birdcalls").is_ok());
        assert!(Id::new("device_birdcalls").is_ok());
        assert!(Id::new("device!birdcalls").is_err());
        assert!(Id::new("device/birdcalls").is_err());
        assert!(Id::new("日本").is_err());
    }

    #[test]
    fn test_id_with_room_new() {
        assert!(IdWithRoom::new("room", "device").is_ok());
        assert!(IdWithRoom::new("room", "device-birdcalls").is_ok());
        assert!(IdWithRoom::new("room", "device_birdcalls").is_ok());
        assert!(IdWithRoom::new("room", "device!birdcalls").is_err());
        assert!(IdWithRoom::new("room", "device/birdcalls").is_err());
        assert!(IdWithRoom::new("room", "日本").is_err());
    }

    #[test]
    fn test_id_serialize() {
        let id = Id::new("hot_water").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        assert_eq!(s, "\"hot_water\"");
    }

    #[test]
    fn test_id_deserialize_valid() {
        let id: Id = serde_json::from_str("\"hot_water\"").unwrap();
        assert_eq!(
            id,
            Id {
                device: "hot_water".to_string()
            }
        );
    }

    #[test]
    fn test_id_deserialize_invalid() {
        assert!(serde_json::from_str::<Id>("\"hot water!\"").is_err());
        assert!(serde_json::from_str::<Id>("\"room/device\"").is_err());
    }

    fn assert_id_with_room_deserialize(input: &str, room: &str, device: &str) {
        let id: IdWithRoom = serde_json::from_str(&format!("\"{input}\"")).unwrap();
        assert_eq!(
            id,
            IdWithRoom {
                room: room.to_string(),
                device: device.to_string(),
            }
        );
    }

    #[test]
    fn test_id_with_room_deserialize_valid() {
        assert_id_with_room_deserialize("room/device", "room", "device");
        assert_id_with_room_deserialize("living_room/hot_water", "living_room", "hot_water");
    }

    fn assert_id_with_room_deserialize_error(input: &str) {
        assert!(
            serde_json::from_str::<IdWithRoom>(&format!("\"{input}\"")).is_err(),
            "expected error for {input:?}"
        );
    }

    #[test]
    fn test_id_with_room_deserialize_errors() {
        assert_id_with_room_deserialize_error("nodevice"); // no slash
        assert_id_with_room_deserialize_error("room/device/extra"); // more than one slash
        assert_id_with_room_deserialize_error("room/"); // empty device
        assert_id_with_room_deserialize_error("/device"); // empty room
        assert_id_with_room_deserialize_error("room/hot water!"); // invalid chars
    }

    #[test]
    fn test_id_with_room_serialize() {
        let id = IdWithRoom::new("room", "device").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        assert_eq!(s, "\"room/device\"");
    }

    #[test]
    fn test_id_serde_roundtrip() {
        let id = Id::new("hot_water").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        let back: Id = serde_json::from_str(&s).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn test_id_with_room_serde_roundtrip() {
        let id = IdWithRoom::new("living_room", "hot_water").unwrap();
        let s = serde_json::to_string(&id).unwrap();
        let back: IdWithRoom = serde_json::from_str(&s).unwrap();
        assert_eq!(id, back);
    }
}
