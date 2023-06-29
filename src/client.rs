use bytes::Bytes;
use eyre::{bail, eyre, Error};
use futures::{
    sink::{Sink, SinkExt},
    stream::{Stream, StreamExt, TryStreamExt},
};
use std::{pin::pin, time::Duration};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::packets::{Codec, Extension, NoResponse, PublicKey, Request, Response, UpstreamList};

#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) struct Client {
    pub(crate) path: String,
}

impl Client {
    #[tracing::instrument(fields(path = ?path.as_ref()))]
    pub(crate) fn new(path: impl AsRef<str>) -> Self {
        Client {
            path: path.as_ref().to_owned(),
        }
    }

    #[culpa::throws]
    async fn connect(
        &self,
    ) -> impl Stream<Item = Result<Response, Error>> + Sink<Request, Error = Error> {
        Framed::new(
            UnixStream::connect(&self.path).await?,
            Codec::<Response, Request>::new(),
        )
        .inspect_ok(|response| tracing::debug!(?response, "received"))
        .with(|request| {
            tracing::debug!(?request, "sending");
            async move { Ok::<_, Error>(request) }
        })
    }

    #[culpa::throws]
    async fn send(&self, request: Request, timeout: Duration) -> Response {
        let mut stream = pin!(self.connect().await?);
        stream.send(request).await?;
        tokio::time::timeout(timeout, stream.next())
            .await?
            .ok_or(eyre!("no response from server"))??
    }

    #[culpa::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn request_identities(&self) -> Vec<PublicKey> {
        // The windows agent at least can be quite slow even when it only has a single identity to
        // return....
        match self
            .send(Request::RequestIdentities, Duration::from_secs(5))
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

    #[culpa::throws]
    #[tracing::instrument(fields(?self.path), skip(self, blob, data, flags))]
    pub(crate) async fn sign_request(&self, blob: Bytes, data: Bytes, flags: u32) -> Option<Bytes> {
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

    #[culpa::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn list_upstreams(&self) -> Vec<String> {
        self.send(
            Request::Extension(Extension::ListUpstreams),
            Duration::from_secs(1),
        )
        .await?
        .parse_extension::<UpstreamList>()?
        .into()
    }

    #[culpa::throws]
    #[tracing::instrument(fields(?self.path), skip(self))]
    pub(crate) async fn add_upstream(&self, path: String) {
        self.send(
            Request::Extension(Extension::AddUpstream { path }),
            Duration::from_secs(1),
        )
        .await?
        .parse_extension::<NoResponse>()?;
    }
}
