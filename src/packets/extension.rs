use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode,
};

#[derive(Debug)]
pub(crate) enum Extension {
    AddUpstream { path: String },
    Unknown { kind: Bytes, contents: Bytes },
}

impl Extension {
    #[fehler::throws]
    pub(crate) fn parse(kind: Bytes, mut contents: Bytes) -> Self {
        let extension = match &kind[..] {
            b"add-upstream@nemo157.com" => {
                let path = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing path"))?;
                let path = String::from_utf8(Vec::from(path))?;
                Self::AddUpstream { path }
            }
            _ => {
                let contents = contents.split_to(contents.len());
                Self::Unknown { kind, contents }
            }
        };
        if !contents.is_empty() {
            bail!("data remaining after end of message");
        }
        extension
    }

    pub(crate) fn kind(&self) -> &[u8] {
        match self {
            Self::AddUpstream { .. } => b"add-upstream@nemo157.com",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl Encode for Extension {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_string(self.kind())?;
        match self {
            Self::AddUpstream { path } => {
                dst.try_put_string(path.as_bytes())?;
            }
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        4 + self.kind().len()
            + match self {
                Self::AddUpstream { path } => 4 + path.len(),
                Self::Unknown { contents, .. } => contents.len(),
            }
    }
}
