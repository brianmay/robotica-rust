//! This module contains the protobuf encoders/decoders
#![allow(missing_docs)]

use bytes::{Bytes, BytesMut};
use prost::EncodeError;
use prost::Message;

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
    fn decode(buf: &[u8]) -> Result<Self, prost::DecodeError>;
}

impl<M: ProtobufIntoFrom> ProtobufEncoderDecoder for M {
    fn encode(self) -> Result<Bytes, EncodeError> {
        let value = self.into_protobuf();
        let mut buf = BytesMut::with_capacity(value.encoded_len());
        value.encode(&mut buf)?;
        Ok(buf.into())
    }

    fn decode(buf: &[u8]) -> Result<Self, prost::DecodeError> {
        let value = M::Protobuf::decode(buf)?;
        let value =
            M::from_protobuf(value).ok_or_else(|| prost::DecodeError::new("Invalid value"))?;
        Ok(value)
    }
}
