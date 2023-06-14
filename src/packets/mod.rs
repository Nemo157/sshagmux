use bytes::{Bytes, BytesMut};
use eyre::Error;

mod codec;
mod request;
mod response;

pub(crate) trait Parse: Sized {
    #[fehler::throws]
    fn parse(kind: u8, contents: Bytes) -> Self;
}

pub(crate) trait Encode: Sized {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut);
    fn encoded_length_estimate(&self) -> usize;
}

pub(crate) use self::{codec::Codec, request::Request, response::Response};
