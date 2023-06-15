use clap::Parser;
use eyre::{eyre, Error};
use futures::future::{AbortHandle, Abortable, Aborted, FutureExt as _, TryFutureExt as _};
use std::sync::Arc;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, EnvFilter};

mod app;
mod client;
mod error;
mod net;
mod packets;
mod server;
mod upstream;

#[fehler::throws]
fn main() {
    color_eyre::install()?;

    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .with_env_var("SSHAGMUX_LOG")
                    .from_env()?,
            )
            .with_writer(std::io::stderr)
            .compact()
            .finish()
            .with(tracing_error::ErrorLayer::default()),
    )?;

    let (handle1, reg1) = AbortHandle::new_pair();
    let (handle2, reg2) = AbortHandle::new_pair();

    ctrlc::set_handler(move || {
        if !handle1.is_aborted() {
            tracing::info!("initial SIGINT, shutdown requested");
            handle1.abort();
            std::thread::spawn({
                let handle2 = handle2.clone();
                move || {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    tracing::warn!("shutdown timeout, hard shutdown requested");
                    handle2.abort();
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    tracing::error!("shutdown timeout, exiting");
                    std::process::exit(1);
                }
            });
        } else if !handle2.is_aborted() {
            tracing::warn!("repeat SIGINT, hard shutdown requested");
            handle2.abort();
        } else {
            tracing::error!("repeat SIGINT, exiting");
            std::process::exit(1);
        }
    })?;

    let context = Arc::new(app::Context::new(
        Abortable::new(futures::future::pending::<()>(), reg1).map(|_| ()),
    ));

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(
            Abortable::new(app::App::parse().run(context), reg2)
                .map_err(|Aborted| eyre!("clean shutdown failed")),
        )??;
}
