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
    packets::{Codec, Extension, ExtensionResponse, Request, Response},
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
                messages
                    .send(Response::Identities {
                        keys: context.upstream.request_identities().await?,
                    })
                    .await?;
            }
            Request::Extension(Extension::AddUpstream { path, nickname }) => {
                match Client::new(&path).await.context("failed to connect") {
                    Ok(client) => {
                        context.upstream.add(&nickname, client).await;
                        messages.send(Response::SUCCESS).await?;
                    }
                    Err(e) => {
                        tracing::warn!("{e:?}");
                        messages
                            .send(Response::Extension(ExtensionResponse::Error(e.into())))
                            .await?;
                    }
                }
            }
            Request::Extension(Extension::ListUpstreams) => {
                messages
                    .send(Response::Extension(ExtensionResponse::UpstreamList(
                        context.upstream.list().into(),
                    )))
                    .await?;
            }
            _ => {
                tracing::warn!(kind = message.kind(), "received unsupported message kind");
                messages.send(Response::FAILURE).await?;
            }
        }
    }

    tracing::info!("client connection closed");
}
