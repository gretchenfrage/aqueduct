//! Tokio utility.

use std::future::Future;
use tokio::task::{
    spawn,
    AbortHandle,
};


/// Wrapper around tokio task that aborts if dropped.
pub struct AbortOnDrop(AbortHandle);

impl AbortOnDrop {
    /// Spawn a tokio task and wrap with self.
    pub fn spawn<F>(f: F) -> Self
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        AbortOnDrop(spawn(f).abort_handle())
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
