use bytes::BytesMut;
use eyre::{bail, Context, Error};
use tokio_util::codec::{Decoder, Encoder};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, Parse,
};

#[derive(Debug)]
pub(crate) struct Codec<O: Parse, I: Encode> {
    length: Option<usize>,
    kind: Option<u8>,
    errored: bool,
    _output: std::marker::PhantomData<fn() -> O>,
    _input: std::marker::PhantomData<fn(I)>,
}

impl<O: Parse, I: Encode> Default for Codec<O, I> {
    fn default() -> Self {
        Self {
            length: Default::default(),
            kind: Default::default(),
            errored: Default::default(),
            _output: Default::default(),
            _input: Default::default(),
        }
    }
}

impl<O: Parse, I: Encode> Codec<O, I> {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl<O: Parse, I: Encode> Decoder for Codec<O, I> {
    type Item = O;
    type Error = Error;

    #[culpa::throws]
    fn decode(&mut self, src: &mut BytesMut) -> Option<Self::Item> {
        if self.errored {
            bail!("something went wrong previously and we can't resynchronize");
        }

        if self.length.is_none() {
            if let Some(length) = src.try_get_u32_be() {
                match usize::try_from(length) {
                    Ok(0) => {
                        self.errored = true;
                        bail!("message must be at least 1 byte for message type");
                    }
                    Ok(length) => {
                        self.length = Some(length);
                    }
                    Err(_) => {
                        self.errored = true;
                        bail!("length doesn't fit in usize");
                    }
                }
            }
        }
        let Some(length) = self.length else { return None; };

        // `type` in the spec
        if self.kind.is_none() {
            self.kind = src.try_get_u8();
        }
        let Some(kind) = self.kind else { return None; };

        if src.len() < length - 1 {
            return None;
        };
        let contents = src.split_to(length - 1).freeze();

        self.length = None;
        self.kind = None;

        Some(O::parse(kind, contents)?)
    }
}

impl<O: Parse, I: Encode> Encoder<I> for Codec<O, I> {
    type Error = Error;

    #[culpa::throws]
    fn encode(&mut self, msg: I, dst: &mut BytesMut) {
        // reserve space so that the unsplit's below will be noops
        dst.reserve(msg.encoded_length_estimate() + 4);
        // presumably the input is empty, but just in case split any existing data
        let mut length_buffer = dst.split_off(dst.len());
        // reserve space to write the length in the end
        length_buffer.try_put_u32_be(0)?;
        // ensure the message can't write over the length
        let mut msg_buffer = length_buffer.split_off(length_buffer.len());
        msg.encode_to(&mut msg_buffer)?;
        length_buffer.clear();
        let length = u32::try_from(msg_buffer.len()).context("length did not fit in u32")?;
        length_buffer.try_put_u32_be(length)?;
        length_buffer.unsplit(msg_buffer);
        dst.unsplit(length_buffer);
    }
}
