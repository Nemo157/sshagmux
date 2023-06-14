use eyre::{Context as _, Error};

use futures::{
    sink::SinkExt,
    stream::{StreamExt, TryStreamExt},
};
use std::{pin::pin, sync::Arc};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::{
    app::Context,
    client::Client,
    packets::{Codec, Extension, Request, Response},
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
            Request::Extension(Extension::AddUpstream { path }) => {
                match Client::new(&path).await.context("failed to connect") {
                    Ok(client) => {
                        context.clients.lock().await.push(client);
                        messages.send(Response::SUCCESS).await?;
                    }
                    Err(e) => {
                        tracing::warn!("{e:?}");
                        messages.send(Response::EXTENSION_FAILURE).await?;
                    }
                }
            }
            _ => {
                tracing::warn!(kind = message.kind(), "received unsupported message kind");
                messages.send(Response::FAILURE).await?;
            }
        }
    }

    tracing::info!("client connection closed");
}
