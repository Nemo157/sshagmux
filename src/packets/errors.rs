use super::util::{BytesExt, BytesMutExt};
use bytes::{Bytes, BytesMut};
use eyre::{eyre, Error};

pub(crate) trait ErrorExt: Sized {
    #[fehler::throws]
    fn encode(&self) -> Bytes;

    #[fehler::throws]
    fn decode(bytes: Bytes) -> Self;
}

impl ErrorExt for Error {
    #[fehler::throws]
    fn encode(&self) -> Bytes {
        let mut bytes = BytesMut::new();
        bytes.try_put_u32_be(u32::try_from(self.chain().len())?)?;
        for cause in self.chain().rev() {
            bytes.try_put_string(cause.to_string().as_ref())?;
        }
        bytes.freeze()
    }

    #[fehler::throws]
    fn decode(mut bytes: Bytes) -> Self {
        let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
        let mut next = |i| {
            Result::<_, Error>::Ok(String::from_utf8(Vec::from(
                bytes
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing message {i}"))?,
            ))?)
        };
        let mut error = Self::msg(next(0)?);
        for i in 1..length {
            error = error.wrap_err(next(i)?);
        }
        error
    }
}
