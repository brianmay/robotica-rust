//! This module contains the protobuf encoders/decoders
#![allow(missing_docs)]

use bytes::{Bytes, BytesMut};
use prost::EncodeError;
use prost::Message;
use std::fmt;

/// Error type for protobuf decoding operations
#[derive(Debug)]
pub enum ProtobufDecodeError {
    /// Error from prost decoding
    DecodeError(prost::DecodeError),
    /// Invalid value after successful decoding
    InvalidValue,
}

impl fmt::Display for ProtobufDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeError(e) => write!(f, "Protobuf decode error: {}", e),
            Self::InvalidValue => write!(f, "Invalid value"),
        }
    }
}

impl std::error::Error for ProtobufDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DecodeError(e) => Some(e),
            Self::InvalidValue => None,
        }
    }
}

impl From<prost::DecodeError> for ProtobufDecodeError {
    fn from(err: prost::DecodeError) -> Self {
        Self::DecodeError(err)
    }
}

pub(super) trait ProtobufIntoFrom: Sized {
    type Protobuf: Message + Default;
    fn into_protobuf(self) -> Self::Protobuf;
    fn from_protobuf(src: Self::Protobuf) -> Option<Self>;
}

pub trait ProtobufEncoderDecoder: Sized {
    /// Encode the value into a protobuf message.
    ///
    /// # Errors
    ///
    /// Returns an error if the value could not be encoded.
    fn encode(self) -> Result<Bytes, EncodeError>;

    /// Decode the value from a protobuf message.
    ///
    /// # Errors
    ///
    /// Returns an error if the value could not be decoded.
    fn decode(buf: &[u8]) -> Result<Self, ProtobufDecodeError>;
}

impl<M: ProtobufIntoFrom> ProtobufEncoderDecoder for M {
    fn encode(self) -> Result<Bytes, EncodeError> {
        let value = self.into_protobuf();
        let mut buf = BytesMut::with_capacity(value.encoded_len());
        value.encode(&mut buf)?;
        Ok(buf.into())
    }

    fn decode(buf: &[u8]) -> Result<Self, ProtobufDecodeError> {
        let value = M::Protobuf::decode(buf)?;
        let value = M::from_protobuf(value).ok_or(ProtobufDecodeError::InvalidValue)?;
        Ok(value)
    }
}
