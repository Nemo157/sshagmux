use eyre::{bail, Context as _, Error};

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
    packets::{Codec, Extension, ExtensionResponse, Request, Response},
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

            Request::Extension(Extension::AddUpstreamV3(upstream)) => {
                let client = Client::from(upstream.clone());
                match async {
                    if Some(upstream.path.as_ref()) == context.path.borrow().as_deref() {
                        bail!("attempted to add self as upstream");
                    }
                    tracing::info!(%upstream.path, upstream.forward_adds, "adding upstream");
                    client
                        .request_identities()
                        .await
                        .context("failed to test connection")?;
                    Ok(())
                }
                .await
                {
                    Ok(()) => {
                        context.upstreams.add(client).await;
                        messages.send(Response::SUCCESS).await?;
                    }
                    Err(e) => {
                        tracing::warn!("sending error back to client: {e:?}");
                        messages
                            .send(Response::ExtensionResponse(ExtensionResponse::ErrorMsgV2(
                                e.into(),
                            )))
                            .await?;
                    }
                }
            }

            Request::Extension(Extension::ListUpstreamsV3) => {
                tracing::info!("processing upstreams v3 request");
                let upstreams = context.upstreams.list();
                messages
                    .send(Response::ExtensionResponse(
                        ExtensionResponse::UpstreamListV3(upstreams),
                    ))
                    .await?;
            }

            Request::Extension(Extension::Query) => {
                tracing::info!("processing query (extensions) request");
                messages
                    .send(Response::ExtensionResponse(ExtensionResponse::Query(
                        // TODO: How to couple this to the actually implemented extensions
                        vec![
                            "add-upstream-v3@nemo157.com".to_owned(),
                            "list-upstreams-v3@nemo157.com".to_owned(),
                            "query".to_owned(),
                        ],
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
