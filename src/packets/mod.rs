use bytes::{Bytes, BytesMut};
use eyre::Error;

mod codec;
pub(crate) mod extension;
mod request;
mod response;
mod util;

pub(crate) use self::{
    codec::Codec,
    extension::{Extension, ExtensionResponse},
    request::Request,
    response::Response,
};

#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) struct PublicKey {
    pub(crate) blob: Bytes,
    pub(crate) comment: Bytes,
}

pub(crate) trait Parse: Sized {
    #[culpa::throws]
    fn parse(kind: u8, contents: Bytes) -> Self;
}

pub(crate) trait Encode: Sized {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut);
    fn encoded_length_estimate(&self) -> usize;
}
