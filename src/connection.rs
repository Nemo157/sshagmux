use eyre::Error;

use futures::{
    sink::SinkExt,
    stream::{StreamExt, TryStreamExt},
};
use std::{future::Future, pin::pin};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::packets::{Codec, Response};

#[fehler::throws]
pub(crate) async fn handle(stream: UnixStream, shutdown: impl Future<Output = ()>) {
    tracing::info!("new connection");

    let mut messages = pin!(Framed::new(stream, Codec::new())
        .take_until(shutdown)
        .inspect_ok(|request| tracing::debug!(?request, "received"))
        .with(|response| {
            tracing::debug!(?response, "sending");
            async move { Ok::<_, Error>(response) }
        }));
    while let Some(message) = messages.next().await.transpose()? {
        messages.send(Response::Failure).await?;
    }

    tracing::info!("connection closed");
}
