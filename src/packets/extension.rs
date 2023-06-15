use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode,
};

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

    #[fehler::throws]
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

    #[fehler::throws]
    fn try_from(bytes: &mut Bytes) -> Self {
        let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
        let next = |i| {
            Result::<_, Error>::Ok(String::from_utf8(Vec::from(
                bytes
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing message {i}"))?,
            ))?)
        };
        ErrorMsg {
            messages: (0..length).map(next).collect::<Result<_, Error>>()?,
        }
    }
}

#[derive(Debug)]
pub(crate) struct UpstreamList {
    pub(crate) upstreams: Vec<(String, String)>,
}

impl From<Vec<(String, String)>> for UpstreamList {
    fn from(upstreams: Vec<(String, String)>) -> Self {
        Self { upstreams }
    }
}

impl From<UpstreamList> for Vec<(String, String)> {
    fn from(list: UpstreamList) -> Self {
        list.upstreams
    }
}

impl TryFrom<&mut Bytes> for UpstreamList {
    type Error = Error;

    #[fehler::throws]
    fn try_from(bytes: &mut Bytes) -> Self {
        let length = usize::try_from(bytes.try_get_u32_be().ok_or(eyre!("missing length"))?)?;
        let mut next = |i| {
            Result::<_, Error>::Ok(String::from_utf8(Vec::from(
                bytes
                    .try_get_string()
                    .ok_or_else(|| eyre!("missing message {i}"))?,
            ))?)
        };
        UpstreamList {
            upstreams: (0..length)
                .map(|i| Ok((next(2 * i)?, next(2 * i + 1)?)))
                .collect::<Result<_, Error>>()?,
        }
    }
}

#[derive(Debug)]
pub(crate) struct NoResponse;

impl TryFrom<&mut Bytes> for NoResponse {
    type Error = Error;

    #[fehler::throws]
    fn try_from(_bytes: &mut Bytes) -> Self {
        Self
    }
}

#[derive(Debug)]
pub(crate) enum Extension {
    AddUpstream { nickname: String, path: String },
    ListUpstreams,
    Unknown { kind: Bytes, contents: Bytes },
}

#[derive(Debug)]
pub(crate) enum ExtensionResponse {
    Error(ErrorMsg),
    UpstreamList(UpstreamList),
}

impl Extension {
    #[fehler::throws]
    pub(crate) fn parse(kind: Bytes, mut contents: Bytes) -> Self {
        let extension = match &kind[..] {
            b"add-upstream@nemo157.com" => {
                let nickname = String::from_utf8(Vec::from(
                    contents
                        .try_get_string()
                        .ok_or_else(|| eyre!("missing nickname"))?,
                ))?;
                let path = String::from_utf8(Vec::from(
                    contents
                        .try_get_string()
                        .ok_or_else(|| eyre!("missing path"))?,
                ))?;
                Self::AddUpstream { nickname, path }
            }
            b"list-upstreams@nemo157.com" => Self::ListUpstreams,
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
            Self::ListUpstreams => b"list-upstreams@nemo157.com",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl ExtensionResponse {
    pub(crate) fn kind(&self) -> u8 {
        match self {
            Self::Error(..) => super::response::SSH_AGENT_EXTENSION_FAILURE,
            Self::UpstreamList(..) => super::response::SSH_AGENT_SUCCESS,
        }
    }
}

impl Encode for Extension {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_string(self.kind())?;
        match self {
            Self::AddUpstream { nickname, path } => {
                dst.try_put_string(nickname.as_bytes())?;
                dst.try_put_string(path.as_bytes())?;
            }
            Self::ListUpstreams => {}
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        4 + self.kind().len()
            + match self {
                Self::AddUpstream { nickname, path } => 4 + nickname.len() + path.len(),
                Self::ListUpstreams => 0,
                Self::Unknown { contents, .. } => contents.len(),
            }
    }
}

impl Encode for ExtensionResponse {
    #[fehler::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        match self {
            Self::Error(ErrorMsg { messages }) => {
                dst.try_put_u32_be(u32::try_from(messages.len())?)?;
                for message in messages {
                    dst.try_put_string(message.as_bytes())?;
                }
            }
            Self::UpstreamList(UpstreamList { upstreams }) => {
                dst.try_put_u32_be(u32::try_from(upstreams.len())?)?;
                for (nickname, path) in upstreams {
                    dst.try_put_string(nickname.as_bytes())?;
                    dst.try_put_string(path.as_bytes())?;
                }
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        match self {
            Self::Error(ErrorMsg { messages }) => {
                4 + messages.iter().map(|m| 4 + m.len()).sum::<usize>()
            }
            Self::UpstreamList(UpstreamList { upstreams }) => {
                4 + upstreams
                    .iter()
                    .map(|(n, p)| 4 + n.len() + 4 + p.len())
                    .sum::<usize>()
            }
        }
    }
}
