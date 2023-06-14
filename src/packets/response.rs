use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, Parse, PublicKey,
};

const SSH_AGENT_FAILURE: u8 = 5;
const SSH_AGENT_SUCCESS: u8 = 6;
const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
const SSH_AGENT_EXTENSION_FAILURE: u8 = 28;
/*
const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
*/

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
pub(crate) enum Response {
    Success { contents: Bytes },
    Failure { contents: Bytes },
    Identities { keys: Vec<PublicKey> },
    ExtensionFailure { contents: Bytes },
    Unknown { kind: u8, contents: Bytes },
}

impl Response {
    /// A `Response::Failure` with no additional contents
    #[allow(clippy::declare_interior_mutable_const)] // It's not visibly interior mutable
    pub(crate) const FAILURE: Self = Self::Failure {
        contents: Bytes::from_static(b""),
    };

    pub(crate) fn kind(&self) -> u8 {
        match self {
            Self::Success { .. } => SSH_AGENT_SUCCESS,
            Self::Failure { .. } => SSH_AGENT_FAILURE,
            Self::Identities { .. } => SSH_AGENT_IDENTITIES_ANSWER,
            Self::ExtensionFailure { .. } => SSH_AGENT_EXTENSION_FAILURE,
            Self::Unknown { kind, .. } => *kind,
        }
    }
}

impl Parse for Response {
    #[fehler::throws]
    fn parse(kind: u8, mut contents: Bytes) -> Self {
        let response = match kind {
            SSH_AGENT_FAILURE => {
                let contents = contents.split_to(contents.len());
                Self::Failure { contents }
            }
            SSH_AGENT_SUCCESS => {
                let contents = contents.split_to(contents.len());
                Self::Success { contents }
            }
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
                Self::Identities { keys }
            }
            SSH_AGENT_EXTENSION_FAILURE => {
                let contents = contents.split_to(contents.len());
                Self::ExtensionFailure { contents }
            }
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

impl Encode for Response {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_u8(self.kind())?;
        match self {
            Self::Success { contents }
            | Self::Failure { contents }
            | Self::ExtensionFailure { contents }
            | Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
            Self::Identities { keys } => {
                dst.try_put_u32_be(u32::try_from(keys.len())?)?;
                for key in keys {
                    dst.try_put_string(key.blob)?;
                    dst.try_put_string(key.comment)?;
                }
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        1 + match self {
            Self::Success { contents }
            | Self::Failure { contents }
            | Self::ExtensionFailure { contents }
            | Self::Unknown { contents, .. } => contents.len(),
            Self::Identities { keys } => {
                4 + keys
                    .iter()
                    .map(|k| 4 + k.blob.len() + 4 + k.comment.len())
                    .sum::<usize>()
            }
        }
    }
}
