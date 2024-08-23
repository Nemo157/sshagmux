use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, ExtensionResponse, Parse, PublicKey,
};

pub(super) const SSH_AGENT_FAILURE: u8 = 5;
pub(super) const SSH_AGENT_SUCCESS: u8 = 6;
pub(super) const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
pub(super) const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
pub(super) const SSH_AGENT_EXTENSION_FAILURE: u8 = 28;
pub(super) const SSH_AGENT_EXTENSION_RESPONSE: u8 = 29;

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
pub(crate) enum Response {
    Success { contents: Bytes },
    Failure { contents: Bytes },
    Identities { keys: Vec<PublicKey> },
    SignResponse { signature: Bytes },
    ExtensionFailure,
    ExtensionResponse(ExtensionResponse),
    Unknown { kind: u8, contents: Bytes },
}

impl Response {
    /// A `Response::Success` with no additional contents
    #[allow(clippy::declare_interior_mutable_const)] // It's not visibly interior mutable
    pub(crate) const SUCCESS: Self = Self::Success {
        contents: Bytes::from_static(b""),
    };

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
            Self::SignResponse { .. } => SSH_AGENT_SIGN_RESPONSE,
            Self::ExtensionFailure { .. } => SSH_AGENT_EXTENSION_FAILURE,
            Self::ExtensionResponse(_) => SSH_AGENT_EXTENSION_RESPONSE,
            Self::Unknown { kind, .. } => *kind,
        }
    }
}

impl Parse for Response {
    #[culpa::throws]
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
            SSH_AGENT_SIGN_RESPONSE => {
                let signature = contents
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing signature"))?;
                Self::SignResponse { signature }
            }
            SSH_AGENT_EXTENSION_FAILURE => Self::ExtensionFailure,
            SSH_AGENT_EXTENSION_RESPONSE => {
                let kind = contents
                    .try_get_utf8_string()
                    .ok_or_else(|| eyre!("missing extension type"))??;
                let contents = contents.split_to(contents.len());
                Self::ExtensionResponse(ExtensionResponse::parse(kind, contents)?)
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
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_u8(self.kind())?;
        match self {
            Self::Success { contents }
            | Self::Failure { contents }
            | Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
            Self::ExtensionFailure => {}
            Self::ExtensionResponse(extension) => {
                extension.encode_to(dst)?;
            }
            Self::Identities { keys } => {
                dst.try_put_u32_be(u32::try_from(keys.len())?)?;
                for key in keys {
                    dst.try_put_string(key.blob)?;
                    dst.try_put_string(key.comment)?;
                }
            }
            Self::SignResponse { signature } => {
                dst.try_put_string(signature)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        1 + match self {
            Self::Success { contents }
            | Self::Failure { contents }
            | Self::Unknown { contents, .. } => contents.len(),
            Self::ExtensionFailure => 0,
            Self::ExtensionResponse(extension) => extension.encoded_length_estimate(),
            Self::Identities { keys } => {
                4 + keys
                    .iter()
                    .map(|k| 4 + k.blob.len() + 4 + k.comment.len())
                    .sum::<usize>()
            }
            Self::SignResponse { signature } => 4 + signature.len(),
        }
    }
}
