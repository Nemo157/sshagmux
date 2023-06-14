use eyre::{Error, WrapErr as _};
use futures::stream::{StreamExt as _, TryStreamExt as _};
use std::{future::Future, path::PathBuf};
use tracing::Instrument;

use crate::{error::ErrorExt as _, net, server};

#[derive(Debug, clap::Parser)]
#[command(version, disable_help_subcommand = true)]
pub(crate) struct App {
    #[arg(long, short('a'))]
    bind_address: Option<PathBuf>,
}

impl App {
    #[fehler::throws]
    pub(crate) async fn run(self, shutdown: impl Future<Output = ()> + Clone) {
        let pid = std::process::id();

        tracing::info!(%self, "starting app");

        let (tempdir, bind_address) = if let Some(bind_address) = self.bind_address {
            (None, bind_address)
        } else {
            let tempdir = tempfile::Builder::new()
                .prefix("sshagmux-XXXXXX")
                .tempdir()?;
            let bind_address = tempdir.path().join(format!("agent.{pid}"));
            (Some(tempdir), bind_address)
        };

        println!("SSH_AUTH_SOCK={bind_address:?}; export SSH_AUTH_SOCK; echo Agent pid {pid};");

        let mut listener = net::UnixListener::bind(bind_address)?;

        let mut next_id = 0;
        listener
            .incoming()
            .take_until(shutdown.clone())
            .map_err(|e| e.wrap_err("failed to accept connection"))
            .try_for_each_concurrent(None, |(stream, _addr)| {
                let connection_id = next_id;
                next_id += 1;
                let shutdown = shutdown.clone();
                async move {
                    if let Err(e) = server::handle(stream, shutdown).await {
                        tracing::warn!("{e:?}");
                    }
                    Ok(())
                }
                .instrument(tracing::info_span!("connection", connection_id))
            })
            .await?;

        listener
            .close()
            .context("could not close unix listener")
            .log_warn();
        if let Some(tempdir) = tempdir {
            tempdir
                .close()
                .context("could not close tempdir")
                .log_warn();
        }
    }
}

impl std::fmt::Display for App {
    #[fehler::throws(std::fmt::Error)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) {
        write!(f, "sshagmux")?;
        if let Some(bind_address) = &self.bind_address {
            write!(f, " --bind_address={:?}", bind_address.display())?;
        }
    }
}
