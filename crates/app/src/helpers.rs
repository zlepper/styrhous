use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hash;
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

pub trait SetExt<T> {
    fn toggle(&mut self, value: T);
}

impl<T> SetExt<T> for HashSet<T>
where
    T: Eq + Hash,
{
    fn toggle(&mut self, value: T) {
        if self.contains(&value) {
            self.remove(&value);
        } else {
            self.insert(value);
        }
    }
}
