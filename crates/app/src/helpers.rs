use std::fmt::Display;
use tracing::log;

pub trait ResultExt {
    fn log_if_error(self, ctx: &str);
}

impl<E: Display> ResultExt for Result<(), E> {
    fn log_if_error(self, ctx: &str) {
        if let Err(e) = self {
            log::error!("{}: {}", ctx, e);
        }
    }
}
