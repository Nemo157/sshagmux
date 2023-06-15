use bytes::{Bytes, BytesMut};
use eyre::Error;

mod codec;
mod extension;
mod request;
mod response;
mod util;

pub(crate) use self::{
    codec::Codec,
    extension::{ErrorMsg, Extension, ExtensionResponse, NoResponse, UpstreamList},
    request::Request,
    response::Response,
};

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
