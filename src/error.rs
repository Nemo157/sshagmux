pub(crate) trait ErrorExt {
    fn log_err(self);
    fn log_warn(self);
}

impl<T> ErrorExt for Result<T, eyre::Report> {
    fn log_err(self) {
        if let Err(e) = self {
            tracing::error!("{e:?}");
        }
    }

    fn log_warn(self) {
        if let Err(e) = self {
            tracing::warn!("{e:?}");
        }
    }
}
