use bytes::{Bytes, BytesMut};
use eyre::Error;

mod codec;
mod request;
mod response;
mod util;

#[derive(Debug)]
pub(crate) struct PublicKey {
    pub(crate) blob: Bytes,
    pub(crate) comment: Bytes,
}

pub(crate) trait Parse: Sized {
    #[fehler::throws]
    fn parse(kind: u8, contents: Bytes) -> Self;
}

pub(crate) trait Encode: Sized {
    // TODO: https://github.com/withoutboats/fehler/issues/39
    fn encode_to(self, dst: &mut BytesMut) -> Result<(), Error>;
    fn encoded_length_estimate(&self) -> usize;
}

pub(crate) use self::{codec::Codec, request::Request, response::Response};
