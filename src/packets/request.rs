use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, Extension, Parse,
};

const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
const SSH_AGENTC_ADD_IDENTITY: u8 = 17;
const SSH_AGENTC_REMOVE_IDENTITY: u8 = 18;
const SSH_AGENTC_REMOVE_ALL_IDENTITIES: u8 = 19;
const SSH_AGENTC_EXTENSION: u8 = 27;
/*
const SSH_AGENTC_ADD_ID_CONSTRAINED: u8 = 25;
const SSH_AGENTC_ADD_SMARTCARD_KEY: u8 = 20;
const SSH_AGENTC_REMOVE_SMARTCARD_KEY: u8 = 21;
const SSH_AGENTC_LOCK: u8 = 22;
const SSH_AGENTC_UNLOCK: u8 = 23;
const SSH_AGENTC_ADD_SMARTCARD_KEY_CONSTRAINED: u8 = 26;
*/

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
#[allow(clippy::enum_variant_names)] // following the specification names
pub(crate) enum Request {
    RequestIdentities,
    SignRequest {
        blob: Bytes,
        data: Bytes,
        flags: u32,
    },
    AddIdentity {
        key: (),
    },
    RemoveIdentity {
        blob: Bytes,
    },
    RemoveAllIdentities,
    Extension(Extension),
    Unknown {
        kind: u8,
        contents: Bytes,
    },
}

impl Request {
    pub(crate) fn kind(&self) -> u8 {
        match self {
            Self::RequestIdentities => SSH_AGENTC_REQUEST_IDENTITIES,
            Self::SignRequest { .. } => SSH_AGENTC_SIGN_REQUEST,
            Self::AddIdentity { .. } => SSH_AGENTC_ADD_IDENTITY,
            Self::RemoveIdentity { .. } => SSH_AGENTC_REMOVE_IDENTITY,
            Self::RemoveAllIdentities => SSH_AGENTC_REMOVE_ALL_IDENTITIES,
            Self::Extension(..) => SSH_AGENTC_EXTENSION,
            Self::Unknown { kind, .. } => *kind,
        }
    }
}

impl Parse for Request {
    #[culpa::throws]
    fn parse(kind: u8, mut contents: Bytes) -> Self {
        let response = match kind {
            SSH_AGENTC_REQUEST_IDENTITIES => Self::RequestIdentities,
            SSH_AGENTC_SIGN_REQUEST => {
                let blob = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing blob"))?;
                let data = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing data"))?;
                let flags = contents
                    .try_get_u32_be()
                    .ok_or_else(|| eyre!("missing flags"))?;
                Self::SignRequest { blob, data, flags }
            }
            SSH_AGENTC_ADD_IDENTITY => {
                let _key_type = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing key type"))?;
                bail!("todo parse and discard contents based on type");
            }
            SSH_AGENTC_REMOVE_IDENTITY => {
                let blob = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing blob"))?;
                Self::RemoveIdentity { blob }
            }
            SSH_AGENTC_EXTENSION => {
                let kind = contents
                    .try_get_utf8_string()
                    .ok_or_else(|| eyre!("missing extension type"))??;
                let contents = contents.split_to(contents.len());
                Self::Extension(Extension::parse(kind, contents)?)
            }
            SSH_AGENTC_REMOVE_ALL_IDENTITIES => Self::RemoveAllIdentities,
            _ => {
                let contents = contents.split_to(contents.len());
                Self::Unknown { kind, contents }
            }
        };
        if !contents.is_empty() {
            bail!("data remaining after end of message");
        }
        response
    }
}

impl Encode for Request {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_u8(self.kind())?;
        match self {
            Self::RequestIdentities | Self::RemoveAllIdentities => {}
            Self::SignRequest { blob, data, flags } => {
                dst.try_put_string(blob)?;
                dst.try_put_string(data)?;
                dst.try_put_u32_be(flags)?;
            }
            Self::AddIdentity { .. } => bail!("add identity unsupported"),
            Self::RemoveIdentity { blob } => {
                dst.try_put_string(blob)?;
            }
            Self::Extension(extension) => {
                extension.encode_to(dst)?;
            }
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        1 + match self {
            Self::RequestIdentities | Self::RemoveAllIdentities | Self::AddIdentity { .. } => 0,
            Self::SignRequest { blob, data, .. } => 4 + blob.len() + 4 + data.len() + 4,
            Self::RemoveIdentity { blob } => 4 + blob.len(),
            Self::Extension(extension) => extension.encoded_length_estimate(),
            Self::Unknown { contents, .. } => contents.len(),
        }
    }
}
