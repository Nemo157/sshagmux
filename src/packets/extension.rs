use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode,
};

use crate::upstreams::Upstream;

#[derive(Debug)]
pub(crate) struct ErrorMsg {
    messages: Vec<String>,
}

impl From<Error> for ErrorMsg {
    fn from(error: Error) -> Self {
        Self {
            messages: error.chain().rev().map(|e| e.to_string()).collect(),
        }
    }
}

impl TryFrom<ErrorMsg> for Error {
    type Error = Error;

    #[culpa::throws]
    fn try_from(msg: ErrorMsg) -> Self {
        let mut messages = msg.messages.into_iter();
        let initial = Self::msg(
            messages
                .next()
                .ok_or_else(|| eyre!("missing initial message"))?,
        );
        messages.fold(initial, |acc, message| acc.wrap_err(message))
    }
}

impl TryFrom<&mut Bytes> for ErrorMsg {
    type Error = Error;

    #[culpa::throws]
    fn try_from(bytes: &mut Bytes) -> Self {
        let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
        ErrorMsg {
            messages: (0..length)
                .map(|i| {
                    bytes
                        .try_get_utf8_string()
                        .ok_or_else(|| eyre!("missing message {i}"))?
                })
                .collect::<Result<_, Error>>()?,
        }
    }
}

#[derive(Debug)]
pub(crate) struct UpstreamListV2 {
    pub(crate) upstreams: Vec<Upstream>,
}

impl TryFrom<&mut Bytes> for UpstreamListV2 {
    type Error = Error;

    #[culpa::throws]
    fn try_from(bytes: &mut Bytes) -> Self {
        let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
        UpstreamListV2 {
            upstreams: (0..length)
                .map(|i| {
                    let path = bytes
                        .try_get_utf8_string_rc()
                        .ok_or_else(|| eyre!("missing upstream path {i}"))??;
                    let forward_adds = bytes
                        .try_get_bool()
                        .ok_or_else(|| eyre!("missing upstream forward_adds {i}"))??;
                    Ok(Upstream { path, forward_adds })
                })
                .collect::<Result<_, Error>>()?,
        }
    }
}

#[derive(Debug)]
pub(crate) struct NoResponse;

impl TryFrom<&mut Bytes> for NoResponse {
    type Error = Error;

    #[culpa::throws]
    fn try_from(_bytes: &mut Bytes) -> Self {
        Self
    }
}

#[derive(Debug)]
pub(crate) enum Extension {
    AddUpstreamV2(Upstream),
    ListUpstreamsV2,
    Unknown { kind: String, contents: Bytes },
}

#[derive(Debug)]
pub(crate) enum ExtensionResponse {
    Error(ErrorMsg),
    UpstreamListV2(UpstreamListV2),
}

impl Extension {
    #[culpa::throws]
    pub(crate) fn parse(kind: String, mut contents: Bytes) -> Self {
        let extension = match kind.as_str() {
            "add-upstream-v2@nemo157.com" => {
                let path = contents
                    .try_get_utf8_string_rc()
                    .ok_or_else(|| eyre!("missing path"))??;
                let forward_adds = contents
                    .try_get_bool()
                    .ok_or_else(|| eyre!("missing forward_adds"))??;
                Self::AddUpstreamV2(Upstream { path, forward_adds })
            }
            "list-upstreams-v2@nemo157.com" => Self::ListUpstreamsV2,
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

    pub(crate) fn kind(&self) -> &str {
        match self {
            Self::AddUpstreamV2 { .. } => "add-upstream-v2@nemo157.com",
            Self::ListUpstreamsV2 => "list-upstreams-v2@nemo157.com",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl ExtensionResponse {
    pub(crate) fn kind(&self) -> u8 {
        match self {
            Self::Error(..) => super::response::SSH_AGENT_EXTENSION_FAILURE,
            Self::UpstreamListV2(..) => super::response::SSH_AGENT_SUCCESS,
        }
    }
}

impl Encode for Extension {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_string(self.kind().as_bytes())?;
        match self {
            Self::AddUpstreamV2(upstream) => {
                dst.try_put_string(upstream.path.as_bytes())?;
                dst.try_put_bool(upstream.forward_adds)?;
            }
            Self::ListUpstreamsV2 => {}
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        4 + self.kind().len()
            + match self {
                Self::AddUpstreamV2(upstream) => 4 + upstream.path.len() + 1,
                Self::ListUpstreamsV2 => 0,
                Self::Unknown { contents, .. } => contents.len(),
            }
    }
}

impl Encode for ExtensionResponse {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        match self {
            Self::Error(ErrorMsg { messages }) => {
                dst.try_put_u32_be(u32::try_from(messages.len())?)?;
                for message in messages {
                    dst.try_put_string(message.as_bytes())?;
                }
            }
            Self::UpstreamListV2(UpstreamListV2 { upstreams }) => {
                dst.try_put_u32_be(u32::try_from(upstreams.len())?)?;
                for upstream in upstreams {
                    dst.try_put_string(upstream.path.as_bytes())?;
                    dst.try_put_bool(upstream.forward_adds)?;
                }
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        match self {
            Self::Error(ErrorMsg { messages }) => {
                4 + messages.iter().map(|m| 4 + m.len()).sum::<usize>()
            }
            Self::UpstreamListV2(UpstreamListV2 { upstreams }) => {
                4 + upstreams
                    .iter()
                    .map(|upstream| 4 + upstream.path.len() + 1)
                    .sum::<usize>()
            }
        }
    }
}
