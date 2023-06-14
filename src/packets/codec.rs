use bytes::{Buf, BufMut, BytesMut};
use eyre::{bail, Context, Error};
use tokio_util::codec::{Decoder, Encoder};

use super::{Encode, Parse};

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

    #[fehler::throws]
    fn decode(&mut self, src: &mut BytesMut) -> Option<Self::Item> {
        if self.errored {
            bail!("something went wrong previously and we can't resynchronize");
        }

        if self.length.is_none() && src.len() >= std::mem::size_of::<u32>() {
            match usize::try_from(src.get_u32()) {
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
        let Some(length) = self.length else { return Ok(None); };

        // `type` in the spec
        if self.kind.is_none() {
            self.kind = (src.len() >= std::mem::size_of::<u8>()).then(|| src.get_u8());
        }
        let Some(kind) = self.kind else { return Ok(None); };

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

    #[fehler::throws]
    fn encode(&mut self, msg: I, dst: &mut BytesMut) {
        // reserve space so that the unsplit's below will be noops
        dst.reserve(msg.encoded_length_estimate() + 4);
        // presumably the input is empty, but just in case split any existing data
        let mut length_buffer = dst.split();
        // reserve space to write the length in the end
        length_buffer.put_u32(0);
        // ensure the message can't write over the length
        let mut msg_buffer = length_buffer.split();
        msg.encode_to(&mut msg_buffer)?;
        length_buffer.clear();
        let length = u32::try_from(msg_buffer.len()).context("length did not fit in u32")?;
        length_buffer.put_u32(length);
        length_buffer.unsplit(msg_buffer);
        dst.unsplit(length_buffer);
    }
}
