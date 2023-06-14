use bytes::{Bytes, BytesMut};
use eyre::{eyre, Error};
use std::sync::Arc;

mod codec;
mod errors;
mod extension;
mod request;
mod response;
mod util;

pub(crate) use self::{
    codec::Codec, errors::ErrorExt, extension::Extension, request::Request, response::Response,
};

use self::util::{BytesExt, BytesMutExt};

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

#[fehler::throws]
pub(crate) fn encode_upstreams(upstreams: Vec<(Arc<str>, String)>) -> Bytes {
    let mut bytes = BytesMut::new();
    bytes.try_put_u32_be(u32::try_from(upstreams.len())?)?;
    for (nickname, path) in upstreams {
        bytes.try_put_string(nickname.as_bytes())?;
        bytes.try_put_string(path.as_ref())?;
    }
    bytes.freeze()
}

#[fehler::throws]
pub(crate) fn decode_upstreams(mut bytes: Bytes) -> Vec<(String, String)> {
    let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
    let mut next = |i| {
        Result::<_, Error>::Ok(String::from_utf8(Vec::from(
            bytes
                .try_get_string()
                .ok_or_else(|| eyre!("missing message {i}"))?,
        ))?)
    };
    (0..length)
        .map(|i| Ok((next(2 * i)?, next(2 * i + 1)?)))
        .collect::<Result<_, Error>>()?
}
