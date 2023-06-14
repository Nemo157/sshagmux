use bytes::Bytes;
use eyre::{bail, eyre, Error};

use super::util::BytesExt;

const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
const SSH_AGENTC_ADD_IDENTITY: u8 = 17;
const SSH_AGENTC_REMOVE_IDENTITY: u8 = 18;
const SSH_AGENTC_REMOVE_ALL_IDENTITIES: u8 = 19;
/*
const SSH_AGENTC_ADD_ID_CONSTRAINED: u8 = 25;
const SSH_AGENTC_ADD_SMARTCARD_KEY: u8 = 20;
const SSH_AGENTC_REMOVE_SMARTCARD_KEY: u8 = 21;
const SSH_AGENTC_LOCK: u8 = 22;
const SSH_AGENTC_UNLOCK: u8 = 23;
const SSH_AGENTC_ADD_SMARTCARD_KEY_CONSTRAINED: u8 = 26;
const SSH_AGENTC_EXTENSION: u8 = 27;
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
    Unknown {
        kind: u8,
        contents: Bytes,
    },
}

impl super::Parse for Request {
    #[fehler::throws]
    fn parse(kind: u8, mut contents: Bytes) -> Self {
        let response = match kind {
            SSH_AGENTC_REQUEST_IDENTITIES => Request::RequestIdentities,
            SSH_AGENTC_SIGN_REQUEST => {
                let blob = contents.try_get_string().ok_or(eyre!("missing blob"))?;
                let data = contents.try_get_string().ok_or(eyre!("missing data"))?;
                let flags = contents.try_get_u32_be().ok_or(eyre!("missing flags"))?;
                Request::SignRequest { blob, data, flags }
            }
            SSH_AGENTC_ADD_IDENTITY => {
                let _key_type = contents.try_get_string().ok_or(eyre!("missing key type"))?;
                bail!("todo parse and discard contents based on type");
            }
            SSH_AGENTC_REMOVE_IDENTITY => {
                let blob = contents.try_get_string().ok_or(eyre!("missing blob"))?;
                Request::RemoveIdentity { blob }
            }
            SSH_AGENTC_REMOVE_ALL_IDENTITIES => Request::RemoveAllIdentities,
            _ => {
                let contents = contents.split_to(contents.len());
                Request::Unknown { kind, contents }
            }
        };
        if !contents.is_empty() {
            bail!("data remaining after end of message");
        }
        response
    }
}
