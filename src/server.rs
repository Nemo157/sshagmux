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
                let keys = context.upstream.request_identities().await?;
                messages.send(Response::Identities { keys }).await?;
            }
            Request::SignRequest { blob, data, flags } => {
                let signature = context.upstream.sign_request(blob, data, flags).await;
                messages
                    .send(
                        signature
                            .map(|signature| Response::SignResponse { signature })
                            .unwrap_or(Response::FAILURE),
                    )
                    .await?;
            }
            Request::Extension(Extension::AddUpstream { path }) => {
                let client = Client::new(&path);
                match client
                    .request_identities()
                    .await
                    .context("failed to test connection")
                {
                    Ok(_) => {
                        context.upstream.add(client).await;
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
                let list = context.upstream.list().into();
                messages
                    .send(Response::Extension(ExtensionResponse::UpstreamList(list)))
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
