use eyre::Error;

use futures::{
    sink::SinkExt,
    stream::{StreamExt, TryStreamExt},
};
use std::{future::Future, pin::pin};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::packets::{Codec, Request, Response};

#[fehler::throws]
pub(crate) async fn handle(stream: UnixStream, shutdown: impl Future<Output = ()>) {
    tracing::info!("new client connection");

    let mut messages = pin!(Framed::new(stream, Codec::<Request, Response>::new())
        .take_until(shutdown)
        .inspect_ok(|request| tracing::debug!(?request, "received"))
        .with(|response| {
            tracing::debug!(?response, "sending");
            async move { Ok::<_, Error>(response) }
        }));

    while let Some(_message) = messages.next().await.transpose()? {
        messages.send(Response::Failure).await?;
    }

    tracing::info!("client connection closed");
}
