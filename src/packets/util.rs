use bytes::{Buf, BufMut, Bytes, BytesMut};
use eyre::{bail, Error};

pub(super) trait BytesExt: Sized {
    fn try_get_u8(&mut self) -> Option<u8>;
    fn try_get_u32_be(&mut self) -> Option<u32>;
    fn try_get_string(&mut self) -> Option<Self>;
    fn try_get_utf8_string(&mut self) -> Option<Result<String, Error>>;
}

impl BytesExt for Bytes {
    fn try_get_u8(&mut self) -> Option<u8> {
        (self.len() >= std::mem::size_of::<u8>()).then(|| self.get_u8())
    }

    fn try_get_u32_be(&mut self) -> Option<u32> {
        (self.len() >= std::mem::size_of::<u32>()).then(|| self.get_u32())
    }

    fn try_get_string(&mut self) -> Option<Self> {
        let length = usize::try_from(self.try_get_u32_be()?).ok()?;
        (self.len() >= length).then(|| self.split_to(length))
    }

    fn try_get_utf8_string(&mut self) -> Option<Result<String, Error>> {
        self.try_get_string()
            .map(Vec::from)
            .map(|v| String::from_utf8(v).map_err(Error::from))
    }
}

impl BytesExt for BytesMut {
    fn try_get_u8(&mut self) -> Option<u8> {
        (self.len() >= std::mem::size_of::<u8>()).then(|| self.get_u8())
    }

    fn try_get_u32_be(&mut self) -> Option<u32> {
        (self.len() >= std::mem::size_of::<u32>()).then(|| self.get_u32())
    }

    fn try_get_string(&mut self) -> Option<Self> {
        let length = usize::try_from(self.try_get_u32_be()?).ok()?;
        (self.len() >= length).then(|| self.split_to(length))
    }

    fn try_get_utf8_string(&mut self) -> Option<Result<String, Error>> {
        self.try_get_string()
            .map(Vec::from)
            .map(|v| String::from_utf8(v).map_err(Error::from))
    }
}

pub(super) trait BytesMutExt: Sized {
    #[culpa::throws]
    fn try_put(&mut self, src: impl Buf);
    #[culpa::throws]
    fn try_put_u8(&mut self, n: u8);
    #[culpa::throws]
    fn try_put_u32_be(&mut self, n: u32);
    #[culpa::throws]
    fn try_put_string(&mut self, string: impl Buf);
}

impl BytesMutExt for BytesMut {
    #[culpa::throws]
    fn try_put(&mut self, src: impl Buf) {
        if self.remaining_mut() < src.remaining() {
            bail!("not enough space remaining");
        }
        self.put(src)
    }

    #[culpa::throws]
    fn try_put_u8(&mut self, n: u8) {
        if self.remaining_mut() < std::mem::size_of::<u8>() {
            bail!("not enough space remaining");
        }
        self.put_u8(n)
    }

    #[culpa::throws]
    fn try_put_u32_be(&mut self, n: u32) {
        if self.remaining_mut() < std::mem::size_of::<u32>() {
            bail!("not enough space remaining");
        }
        self.put_u32(n)
    }

    #[culpa::throws]
    fn try_put_string(&mut self, string: impl Buf) {
        self.try_put_u32_be(u32::try_from(string.remaining())?)?;
        self.try_put(string)?;
    }
}
