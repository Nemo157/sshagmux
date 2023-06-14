use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, Parse, PublicKey,
};

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
    Unknown { kind: u8, contents: Bytes },
}

impl Parse for Response {
    #[fehler::throws]
    fn parse(kind: u8, mut contents: Bytes) -> Self {
        let response = match kind {
            SSH_AGENT_FAILURE => Response::Failure,
            SSH_AGENT_SUCCESS => Response::Success,
            SSH_AGENT_IDENTITIES_ANSWER => {
                let length = usize::try_from(
                    contents
                        .try_get_u32_be()
                        .ok_or_else(|| eyre!("missing length"))?,
                )?;
                let keys = std::iter::from_fn(|| {
                    Some(Ok(PublicKey {
                        blob: contents.try_get_string()?,
                        comment: contents.try_get_string()?,
                    }))
                })
                .take(length)
                .collect::<Result<_, Error>>()?;
                Response::Identities { keys }
            }
            _ => {
                let contents = contents.split_to(contents.len());
                Response::Unknown { kind, contents }
            }
        };
        if !contents.is_empty() {
            bail!("data remaining after end of message");
        }
        response
    }
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
            Response::Unknown { kind, contents } => {
                dst.try_put_u8(kind)?;
                dst.try_put(contents)?;
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
            Response::Unknown { contents, .. } => 1 + contents.len(),
        }
    }
}
