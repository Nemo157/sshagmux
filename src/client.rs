use eyre::{bail, eyre, Error};
use futures::{
    sink::{Sink, SinkExt},
    stream::{Stream, StreamExt, TryStreamExt},
};
use std::{pin::Pin, time::Duration};
use tokio::{net::UnixStream, time::timeout};
use tokio_util::codec::Framed;

use crate::packets::{Codec, Extension, NoResponse, PublicKey, Request, Response, UpstreamList};

trait ClientStream: Stream<Item = Result<Response, Error>> + Sink<Request, Error = Error> {}

impl<T: Stream<Item = Result<Response, Error>> + Sink<Request, Error = Error>> ClientStream for T {}

pub(crate) struct Client {
    pub(crate) path: String,
    stream: Pin<Box<dyn ClientStream>>,
}

impl Client {
    #[fehler::throws]
    #[tracing::instrument(fields(path = ?path.as_ref()))]
    pub(crate) async fn new(path: impl AsRef<str>) -> Self {
        Client {
            path: path.as_ref().to_owned(),
            stream: Box::pin(
                Framed::new(
                    UnixStream::connect(path.as_ref()).await?,
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
    pub(crate) async fn list_upstreams(&mut self) -> Vec<(String, String)> {
        self.stream
            .send(Request::Extension(Extension::ListUpstreams))
            .await?;
        timeout(Duration::from_secs(1), self.stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
            .parse_extension::<UpstreamList>()?
            .into()
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
        timeout(Duration::from_secs(3), self.stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
            .parse_extension::<NoResponse>()?;
    }
}
