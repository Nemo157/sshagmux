use eyre::{Error, WrapErr as _};
use futures::stream::{StreamExt as _, TryStreamExt as _};
use std::{future::Future, path::PathBuf};
use tracing::Instrument;

use crate::{connection, net, error::ErrorExt as _};

#[derive(Debug, clap::Parser)]
#[command(version, disable_help_subcommand = true)]
pub(crate) struct App {
    #[arg(long, short('a'))]
    bind_address: Option<PathBuf>,
}

impl App {
    #[fehler::throws]
    #[tracing::instrument(fields(%self), skip(shutdown))]
    pub(crate) async fn run(self, shutdown: impl Future<Output = ()> + Send + 'static) {
        let pid = std::process::id();

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
            .take_until(shutdown)
            .map_err(|e| e.wrap_err("failed to accept connection"))
            .try_for_each_concurrent(None, |(stream, _addr)| {
                let id = next_id;
                next_id += 1;
                connection::handle(stream).instrument(tracing::info_span!("connection", id))
            })
            .await?;

        listener.close().context("could not close unix listener").log_warn();
        if let Some(tempdir) = tempdir {
            tempdir.close().context("could not close tempdir").log_warn();
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
