use eyre::Error;

use futures::{
    sink::SinkExt,
    stream::{StreamExt, TryStreamExt},
};
use std::{pin::pin, sync::Arc};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::{
    app::Context,
    packets::{Codec, Request, Response},
};

#[fehler::throws]
pub(crate) async fn handle(stream: UnixStream, context: Arc<Context>) {
    tracing::info!("new client connection");

    let mut messages = pin!(Framed::new(stream, Codec::<Request, Response>::new())
        .take_until(context.shutdown.clone())
        .inspect_ok(|request| tracing::debug!(?request, "received"))
        .with(|response| {
            tracing::debug!(?response, "sending");
            async move { Ok::<_, Error>(response) }
        }));

    while let Some(message) = messages.next().await.transpose()? {
        match message {
            Request::RequestIdentities => {
                let mut clients = context.clients.lock().await;
                let mut keys = Vec::new();
                for client in &mut clients[..] {
                    keys.extend(client.request_identities().await?);
                }
                messages.send(Response::Identities { keys }).await?;
            }
            Request::SignRequest { .. } => {
                messages.send(Response::Failure).await?;
            }
            Request::AddIdentity { .. } => {
                messages.send(Response::Failure).await?;
            }
            Request::RemoveIdentity { .. } => {
                messages.send(Response::Failure).await?;
            }
            Request::RemoveAllIdentities => {
                messages.send(Response::Failure).await?;
            }
            Request::Unknown { kind, .. } => {
                tracing::warn!(kind, "received unknown message kind");
                messages.send(Response::Failure).await?;
            }
        }
    }

    tracing::info!("client connection closed");
}
