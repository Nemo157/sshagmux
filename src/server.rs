use eyre::{Context as _, Error};

use futures::{
    sink::SinkExt,
    stream::{StreamExt, TryStreamExt},
};
use std::{pin::pin, rc::Rc};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

use crate::{
    app::Context,
    client::Client,
    packets::{Codec, Extension, ExtensionResponse, Request, Response, UpstreamListV2},
};

#[culpa::throws]
pub(crate) async fn handle(stream: UnixStream, context: Rc<Context>) {
    tracing::debug!("new client connection");

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
                tracing::info!("processing identities request");
                let keys = context.upstreams.request_identities().await?;
                messages.send(Response::Identities { keys }).await?;
            }
            Request::AddIdentity { .. }
            | Request::AddIdConstrained { .. }
            | Request::RemoveIdentity { .. }
            | Request::RemoveAllIdentities { .. } => {
                tracing::info!("processing {message:?}");
                let response = context.upstreams.forward_to_adds(message).await?;
                messages.send(response).await?;
            }
            Request::SignRequest { blob, data, flags } => {
                tracing::info!("processing sign request");
                let signature = context.upstreams.sign_request(blob, data, flags).await;
                messages
                    .send(
                        signature
                            .map(|signature| Response::SignResponse { signature })
                            .unwrap_or(Response::FAILURE),
                    )
                    .await?;
            }
            Request::Extension(Extension::AddUpstreamV2(upstream)) => {
                tracing::info!(%upstream.path, upstream.forward_adds, "adding upstream");
                let client = Client::from(upstream);
                match client
                    .request_identities()
                    .await
                    .context("failed to test connection")
                {
                    Ok(_) => {
                        context.upstreams.add(client).await;
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
            Request::Extension(Extension::ListUpstreamsV2) => {
                tracing::info!("processing upstreams v2 request");
                let upstreams = context.upstreams.list();
                messages
                    .send(Response::Extension(ExtensionResponse::UpstreamListV2(
                        UpstreamListV2 { upstreams },
                    )))
                    .await?;
            }
            Request::Extension(extension) => {
                tracing::warn!(
                    kind = extension.kind(),
                    "received unsupported extension kind"
                );
                messages.send(Response::FAILURE).await?;
            }
            message => {
                tracing::warn!(kind = message.kind(), "received unsupported message kind");
                messages.send(Response::FAILURE).await?;
            }
        }
    }

    tracing::debug!("client connection closed");
}
