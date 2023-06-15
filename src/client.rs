use bytes::Bytes;
use eyre::{bail, eyre, Error};
use futures::{
    sink::{Sink, SinkExt},
    stream::{Stream, StreamExt, TryStreamExt},
};
use std::{pin::Pin, time::Duration};
use tokio::net::UnixStream;
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
    async fn send(&mut self, request: Request, timeout: Duration) -> Response {
        self.stream.send(request).await?;
        tokio::time::timeout(timeout, self.stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn request_identities(&mut self) -> Vec<PublicKey> {
        match self
            .send(Request::RequestIdentities, Duration::from_secs(1))
            .await?
        {
            Response::Identities { keys } => keys,
            Response::Failure { .. } => {
                bail!("server returned failure")
            }
            _ => {
                bail!("server returned unexpected response")
            }
        }
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self, blob, data, flags))]
    pub(crate) async fn sign_request(
        &mut self,
        blob: Bytes,
        data: Bytes,
        flags: u32,
    ) -> Option<Bytes> {
        // Needs a long timeout as it may require human interaction
        match self
            .send(
                Request::SignRequest { blob, data, flags },
                Duration::from_secs(60),
            )
            .await?
        {
            Response::SignResponse { signature } => Some(signature),
            // A failure probably just means the agent refused to sign the request, maybe because
            // it's the wrong agent
            Response::Failure { .. } => None,
            _ => {
                bail!("server returned unexpected response")
            }
        }
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn list_upstreams(&mut self) -> Vec<(String, String)> {
        self.send(
            Request::Extension(Extension::ListUpstreams),
            Duration::from_secs(1),
        )
        .await?
        .parse_extension::<UpstreamList>()?
        .into()
    }

    #[fehler::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn add_upstream(&mut self, nickname: String, path: String) {
        self.send(
            Request::Extension(Extension::AddUpstream { nickname, path }),
            Duration::from_secs(1),
        )
        .await?
        .parse_extension::<NoResponse>()?;
    }
}
