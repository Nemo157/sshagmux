use bytes::{Bytes, BytesMut};
use eyre::{bail, eyre, Error};

use super::{
    util::{BytesExt, BytesMutExt},
    Encode,
};

use crate::upstreams::Upstream;

#[derive(Debug)]
pub(crate) struct ErrorMsgV2 {
    messages: Vec<String>,
}

impl From<Error> for ErrorMsgV2 {
    fn from(error: Error) -> Self {
        Self {
            messages: error.chain().rev().map(|e| e.to_string()).collect(),
        }
    }
}

impl TryFrom<ErrorMsgV2> for Error {
    type Error = Error;

    #[culpa::throws]
    fn try_from(msg: ErrorMsgV2) -> Self {
        let mut messages = msg.messages.into_iter();
        let initial = Self::msg(
            messages
                .next()
                .ok_or_else(|| eyre!("missing initial message"))?,
        );
        messages.fold(initial, |acc, message| acc.wrap_err(message))
    }
}

#[derive(Debug)]
pub(crate) enum Extension {
    AddUpstreamV3(Upstream),
    ListUpstreamsV3,
    Query,
    Unknown { kind: String, contents: Bytes },
}

#[derive(Debug)]
pub(crate) enum ExtensionResponse {
    ErrorMsgV2(ErrorMsgV2),
    UpstreamListV3(Vec<Upstream>),
    Query(Vec<String>),
    Unknown { kind: String, contents: Bytes },
}

impl Extension {
    #[culpa::throws]
    pub(crate) fn parse(kind: String, mut contents: Bytes) -> Self {
        let extension = match kind.as_str() {
            "add-upstream-v3@nemo157.com" => {
                let path = contents
                    .try_get_utf8_string_rc()
                    .ok_or_else(|| eyre!("missing path"))??;
                let forward_adds = contents
                    .try_get_bool()
                    .ok_or_else(|| eyre!("missing forward_adds"))??;
                Self::AddUpstreamV3(Upstream { path, forward_adds })
            }
            "list-upstreams-v3@nemo157.com" => Self::ListUpstreamsV3,
            "query" => Self::Query,
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
            Self::AddUpstreamV3 { .. } => "add-upstream-v3@nemo157.com",
            Self::ListUpstreamsV3 => "list-upstreams-v3@nemo157.com",
            Self::Query => "query",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl ExtensionResponse {
    #[culpa::throws]
    pub(crate) fn parse(kind: String, mut contents: Bytes) -> Self {
        let extension = match kind.as_str() {
            "error-msg-v2@nemo157.com" => Self::ErrorMsgV2(ErrorMsgV2 {
                messages: std::iter::from_fn(|| contents.try_get_utf8_string())
                    .collect::<Result<_, Error>>()?,
            }),

            "list-upstreams-v3@nemo157.com" => Self::UpstreamListV3(
                std::iter::from_fn(|| {
                    let path = match contents.try_get_utf8_string_rc() {
                        Some(Ok(path)) => path,
                        Some(Err(err)) => return Some(Err(err)),
                        None => return None,
                    };
                    let forward_adds = match contents.try_get_bool() {
                        Some(Ok(forward_adds)) => forward_adds,
                        Some(Err(err)) => return Some(Err(err)),
                        None => return Some(Err(eyre!("missing upstream forward_adds"))),
                    };
                    Some(Ok(Upstream { path, forward_adds }))
                })
                .collect::<Result<_, Error>>()?,
            ),

            "query" => Self::Query(
                std::iter::from_fn(|| contents.try_get_utf8_string())
                    .collect::<Result<_, Error>>()?,
            ),

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
            Self::ErrorMsgV2(..) => "error-msg-v2@nemo157.com",
            Self::UpstreamListV3(..) => "list-upstreams-v3@nemo157.com",
            Self::Query(..) => "query",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl Encode for Extension {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_string(self.kind().as_bytes())?;
        match self {
            Self::AddUpstreamV3(upstream) => {
                dst.try_put_string(upstream.path.as_bytes())?;
                dst.try_put_bool(upstream.forward_adds)?;
            }
            Self::ListUpstreamsV3 => {}
            Self::Query => {}
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        4 + self.kind().len()
            + match self {
                Self::AddUpstreamV3(upstream) => 4 + upstream.path.len() + 1,
                Self::ListUpstreamsV3 => 0,
                Self::Query => 0,
                Self::Unknown { contents, .. } => contents.len(),
            }
    }
}

impl Encode for ExtensionResponse {
    #[culpa::throws]
    fn encode_to(self, dst: &mut BytesMut) {
        dst.try_put_string(self.kind().as_bytes())?;
        match self {
            Self::ErrorMsgV2(ErrorMsgV2 { messages }) => {
                for message in messages {
                    dst.try_put_string(message.as_bytes())?;
                }
            }
            Self::UpstreamListV3(upstreams) => {
                for upstream in upstreams {
                    dst.try_put_string(upstream.path.as_bytes())?;
                    dst.try_put_bool(upstream.forward_adds)?;
                }
            }
            Self::Query(extensions) => {
                for extension in extensions {
                    dst.try_put_string(extension.as_bytes())?;
                }
            }
            Self::Unknown { contents, .. } => {
                dst.try_put(contents)?;
            }
        }
    }

    fn encoded_length_estimate(&self) -> usize {
        4 + self.kind().len()
            + match self {
                Self::ErrorMsgV2(ErrorMsgV2 { messages }) => {
                    messages.iter().map(|m| 4 + m.len()).sum::<usize>()
                }
                Self::UpstreamListV3(upstreams) => upstreams
                    .iter()
                    .map(|upstream| 4 + upstream.path.len() + 1)
                    .sum::<usize>(),
                Self::Query(extensions) => extensions
                    .iter()
                    .map(|extension| 4 + extension.len())
                    .sum::<usize>(),
                Self::Unknown { contents, .. } => contents.len(),
            }
    }
}
