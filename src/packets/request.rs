use bytes::Bytes;
use eyre::Error;

/*
const SSH_AGENTC_REQUEST_IDENTITIES: u8 = 11;
const SSH_AGENTC_SIGN_REQUEST: u8 = 13;
const SSH_AGENTC_ADD_IDENTITY: u8 = 17;
const SSH_AGENTC_REMOVE_IDENTITY: u8 = 18;
const SSH_AGENTC_REMOVE_ALL_IDENTITIES: u8 = 19;
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
pub(crate) enum Request {
    Unknown { kind: u8, contents: Bytes },
}

impl super::Parse for Request {
    #[fehler::throws]
    fn parse(kind: u8, contents: Bytes) -> Self {
        match kind {
            _ => Request::Unknown { kind, contents },
        }
    }
}
