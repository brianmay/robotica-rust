//! Common structs shared between robotica-backend and robotica-frontend for websockets
use crate::{mqtt::MqttMessage, user::User, version::Version};

#[cfg(feature = "websockets")]
use crate::{protobuf::ProtobufIntoFrom, protos};

/// Error message sent from the backend to the frontend
#[derive(Debug)]
pub enum WsError {
    /// The user is not authorized to access the websocket
    NotAuthorized,
}

/// Message sent from the backend to the frontend after websocket connected
#[derive(Debug)]
pub enum WsStatus {
    /// The websocket is connected
    Connected {
        /// The user that is connected
        user: User,

        /// The version of the backend
        version: Version,
    },

    /// The websocket is disconnected
    Disconnected(WsError),
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for WsStatus {
    type Protobuf = protos::WsStatus;

    fn into_protobuf(self) -> protos::WsStatus {
        protos::WsStatus {
            status: Some(match self {
                WsStatus::Connected { user, version } => {
                    protos::ws_status::Status::Connected(protos::WsConnected {
                        user: Some(user.into_protobuf()),
                        version: Some(version.into_protobuf()),
                    })
                }
                WsStatus::Disconnected(error) => {
                    protos::ws_status::Status::Disconnected(error as i32)
                }
            }),
        }
    }

    fn from_protobuf(status: protos::WsStatus) -> Option<Self> {
        Some(match status.status? {
            protos::ws_status::Status::Connected(connected) => WsStatus::Connected {
                user: User::from_protobuf(connected.user?)?,
                version: Version::from_protobuf(connected.version?)?,
            },
            protos::ws_status::Status::Disconnected(error) => {
                let err = match error {
                    x if x == protos::WsError::NotAuthorized as i32 => WsError::NotAuthorized,
                    _ => return None,
                };
                WsStatus::Disconnected(err)
            }
        })
    }
}

/// Message sent from the frontend to the backend.
#[derive(Debug)]
pub enum WsCommand {
    /// Frontend wants to subscribe to MQTT topic.
    Subscribe {
        /// MQTT topic to subscribe to.
        topic: String,
    },

    /// Frontend wants to unsubscribe from MQTT topic.
    Unsubscribe {
        /// MQTT topic to unsubscribe from.
        topic: String,
    },

    /// Frontend wants to send a MQTT message.
    Send(MqttMessage),

    /// Keep alive message.
    KeepAlive,
}

#[cfg(feature = "websockets")]
impl ProtobufIntoFrom for WsCommand {
    type Protobuf = protos::WsCommand;

    fn into_protobuf(self) -> Self::Protobuf {
        Self::Protobuf {
            command: Some(match self {
                WsCommand::Subscribe { topic } => {
                    protos::ws_command::Command::Subscribe(protos::WsSubscribe { topic })
                }
                WsCommand::Unsubscribe { topic } => {
                    protos::ws_command::Command::Unsubscribe(protos::WsUnsubscribe { topic })
                }
                WsCommand::Send(message) => {
                    crate::protos::ws_command::Command::Send(protos::WsSend {
                        message: Some(message.into_protobuf()),
                    })
                }
                WsCommand::KeepAlive => {
                    crate::protos::ws_command::Command::KeepAlive(protos::WsKeepAlive {})
                }
            }),
        }
    }

    fn from_protobuf(src: Self::Protobuf) -> Option<Self> {
        Some(match src.command? {
            protos::ws_command::Command::Subscribe(subscribe) => WsCommand::Subscribe {
                topic: subscribe.topic,
            },
            protos::ws_command::Command::Unsubscribe(subscribe) => WsCommand::Unsubscribe {
                topic: subscribe.topic,
            },
            protos::ws_command::Command::Send(protos::WsSend { message }) => {
                WsCommand::Send(MqttMessage::from_protobuf(message?)?)
            }
            protos::ws_command::Command::KeepAlive(_) => WsCommand::KeepAlive,
        })
    }
}
