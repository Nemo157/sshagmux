use bytes::BytesMut;
use eyre::Error;

use super::{util::BytesMutExt, Encode, PublicKey};

const SSH_AGENT_FAILURE: u8 = 5;
const SSH_AGENT_SUCCESS: u8 = 6;
const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
/*
const SSH_AGENT_EXTENSION_FAILURE: u8 = 28;
const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
*/

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
pub(crate) enum Response {
    Success,
    Failure,
    Identities { keys: Vec<PublicKey> },
}

impl Encode for Response {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        match self {
            Response::Success => {
                dst.try_put_u8(SSH_AGENT_SUCCESS)?;
            }
            Response::Failure => {
                dst.try_put_u8(SSH_AGENT_FAILURE)?;
            }
            Response::Identities { keys } => {
                dst.try_put_u8(SSH_AGENT_IDENTITIES_ANSWER)?;
                dst.try_put_u32_be(u32::try_from(keys.len())?)?;
                for key in keys {
                    dst.try_put_string(key.blob)?;
                    dst.try_put_string(key.comment)?;
                }
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        match self {
            Response::Success | Response::Failure => 1,
            Response::Identities { keys } => {
                1 + 4
                    + keys
                        .iter()
                        .map(|k| 4 + k.blob.len() + 4 + k.comment.len())
                        .sum::<usize>()
            }
        }
    }
}
