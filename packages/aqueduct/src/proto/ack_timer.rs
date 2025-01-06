
use tokio::time::{Sleep, sleep};
use std::{
    time::Duration,
    task::{Context, Poll},
    future::Future,
    pin::Pin,
};


const ACK_DELAY: Duration = Duration::from_secs(1);

// timer for letting acks aggregate before sending.
//
// begins in a "stopped" state, where it pends forever. calling start starts the timer unless the
// timer is already running. after a delay, it resolves to the value it was started with, and thus
// transitions back to a stopped state, whereupon it may be started again later.
pub(crate) struct AckTimer<T>(Option<(Sleep, T)>);

impl<T: Copy> Future for AckTimer<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<T> {
        unsafe {
            let this = self.get_unchecked_mut();
            if let &mut Some((ref mut sleep, val)) = &mut this.0 {
                match Pin::new_unchecked(sleep).poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(()) => {
                        this.sleep = None;
                        Poll::Ready(val)
                    }
                }
            } else {
                Poll::Pending
            }
        }
    }
}

impl<T> AckTimer<T> {
    pub(crate) fn new() -> Self {
        AckTimer(None)
    }

    // start the timer if it's not yet started.
    pub(crate) fn start(self: &mut Pin<&mut Self>, val: T) {
        unsafe {
            let this = self.as_mut().get_unchecked_mut();
            if this.0.is_none() {
                this.0 = Some((sleep(ACK_DELAY), val));
            }
        }
    }
}
