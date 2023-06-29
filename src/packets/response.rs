use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Context, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode, ErrorMsg, ExtensionResponse, Parse, PublicKey,
};

pub(super) const SSH_AGENT_FAILURE: u8 = 5;
pub(super) const SSH_AGENT_SUCCESS: u8 = 6;
pub(super) const SSH_AGENT_IDENTITIES_ANSWER: u8 = 12;
pub(super) const SSH_AGENT_SIGN_RESPONSE: u8 = 14;
pub(super) const SSH_AGENT_EXTENSION_FAILURE: u8 = 28;

#[derive(Debug)]
#[allow(dead_code)] // some variants are unused
pub(crate) enum Response {
    Success { contents: Bytes },
    Failure { contents: Bytes },
    Identities { keys: Vec<PublicKey> },
    SignResponse { signature: Bytes },
    // Not actually a different variant, encodes into a `Success`/`ExtensionFailure`, to
    // parse you need to parse the underlying response types from the `contents` of those variants
    // using their `TryFrom<Bytes>` implementations.
    Extension(ExtensionResponse),
    ExtensionFailure { contents: Bytes },
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
            Self::Extension(extension) => extension.kind(),
            Self::ExtensionFailure { .. } => SSH_AGENT_EXTENSION_FAILURE,
            Self::Unknown { kind, .. } => *kind,
        }
    }

    /// Expects self to be one of:
    ///
    ///  * `Success` containing an encoded `T`
    ///  * `Failure` because the agent did not understand the extension
    ///  * `ExtensionFailure` containing an `ErrorMsg`
    ///
    /// because of the third option this can't be used with _any_ extension, only those that use
    /// this way to pass back error messages
    #[culpa::throws]
    pub(crate) fn parse_extension<T: for<'a> TryFrom<&'a mut Bytes, Error = Error>>(self) -> T {
        match self {
            Response::Success { mut contents } => {
                let result = T::try_from(&mut contents)?;
                if !contents.is_empty() {
                    bail!("data remaining after end of message");
                }
                result
            }
            Response::Failure { .. } => {
                bail!("server doesn't understand extension");
            }
            Response::ExtensionFailure { mut contents } => {
                match ErrorMsg::try_from(&mut contents)
                    .and_then(Error::try_from)
                    .context("failed to parse server failure")
                {
                    Ok(error) => {
                        if !contents.is_empty() {
                            tracing::warn!("data remaining after end of error messages");
                        }
                        bail!(error.wrap_err("server returned failure"))
                    }
                    Err(e) => {
                        tracing::warn!("{e:?}");
                        bail!("server returned failure");
                    }
                }
            }
            _ => {
                bail!("server returned unexpected response")
            }
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
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_u8(self.kind())?;
        match self {
            Self::Success { contents }
            | Self::Failure { contents }
            | Self::ExtensionFailure { contents }
            | Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
            Self::Extension(extension) => {
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
            | Self::ExtensionFailure { contents }
            | Self::Unknown { contents, .. } => contents.len(),
            Self::Extension(extension) => extension.encoded_length_estimate(),
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
