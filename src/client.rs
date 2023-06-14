use eyre::{bail, eyre, Context as _, Error};
use futures::{
    sink::{Sink, SinkExt},
    stream::{Stream, StreamExt, TryStreamExt},
};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
    time::Duration,
};
use tokio::{net::UnixStream, time::timeout};
use tokio_util::codec::Framed;

use crate::packets::{Codec, ErrorExt as _, Extension, PublicKey, Request, Response};

trait ClientStream: Stream<Item = Result<Response, Error>> + Sink<Request, Error = Error> {}

impl<T: Stream<Item = Result<Response, Error>> + Sink<Request, Error = Error>> ClientStream for T {}

pub(crate) struct Client {
    path: PathBuf,
    stream: Pin<Box<dyn ClientStream>>,
}

impl Client {
    #[fehler::throws]
    #[tracing::instrument(fields(path = ?path.as_ref()))]
    pub(crate) async fn new(path: impl AsRef<Path>) -> Self {
        Client {
            path: path.as_ref().to_owned(),
            stream: Box::pin(
                Framed::new(
                    UnixStream::connect(path).await?,
                    Codec::<Response, Request>::new(),
                )
                .inspect_ok(|response| tracing::debug!(?response, "received"))
                .with(|request| {
                    tracing::debug!(?request, "sending");
                    async move { Ok::<_, Error>(request) }
                }),
            ),
        }
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn request_identities(&mut self) -> Vec<PublicKey> {
        self.stream.send(Request::RequestIdentities).await?;
        match timeout(Duration::from_secs(1), self.stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
        {
            Response::Failure { .. } => {
                bail!("server returned failure")
            }
            Response::Identities { keys } => keys,
            _ => {
                bail!("server returned unexpected response")
            }
        }
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn add_upstream(&mut self, nickname: String, path: String) {
        self.stream
            .send(Request::Extension(Extension::AddUpstream {
                nickname,
                path,
            }))
            .await?;
        match timeout(Duration::from_secs(3), self.stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
        {
            Response::Success { .. } => {}
            Response::Failure { .. } => {
                bail!("server doesn't understand extension");
            }
            Response::ExtensionFailure { contents } => {
                match Error::decode(contents).context("failed to parse server failure") {
                    Ok(error) => bail!(error.wrap_err("server returned failure")),
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
