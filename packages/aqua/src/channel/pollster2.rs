//! Significantly modified copy of the pollster crate
//!
//! - Adds blocking on with a timeout
//! - Uses an unsafe trait to to store the waker state on the stack rather than in an `Arc`
//! - Futures now passed by `&mut` rather than value

use std::{
    future::Future,
    sync::{Condvar, Mutex},
    task::{Context, Poll, Waker, RawWaker, RawWakerVTable},
    time::{Instant, Duration},
};

/// unsafe trait for futures which provide a `drop_wakers` method which must guarantee that any
/// `std::task::Waker`s that were cloned from `std::task::Context`s the future was `poll`'d with
/// are dropped
pub unsafe trait DropWakers {
    /// drop all wakers that were cloned from contexts this future was polled with
    ///
    /// it is ok if this renders the future un-pollable.
    fn drop_wakers(&mut self);
}

enum SignalState {
    Empty,
    Waiting,
    Notified,
}

struct Signal {
    state: Mutex<SignalState>,
    cond: Condvar,
}

enum Timeout {
    None,
    After(Duration),
    Immediate,
}

impl Signal {
    fn new() -> Self {
        Self {
            state: Mutex::new(SignalState::Empty),
            cond: Condvar::new(),
        }
    }

    fn wait(&self, timeout: &Timeout) -> bool {
        let mut state = self.state.lock().unwrap();
        match *state {
            SignalState::Notified => *state = SignalState::Empty,
            SignalState::Waiting => unreachable!(),
            SignalState::Empty => {
                *state = SignalState::Waiting;
                while let SignalState::Waiting = *state {
                    match timeout {
                        &Timeout::None => {
                            state = self.cond.wait(state).unwrap();
                        }
                        &Timeout::After(dur) => {
                            let (state2, result) = self.cond.wait_timeout(state, dur).unwrap();
                            state = state2;
                            if result.timed_out() {
                                *state = SignalState::Empty;
                                return true;
                            }
                        }
                        &Timeout::Immediate => {
                            *state = SignalState::Empty;
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn notify(&self) {
        let mut state = self.state.lock().unwrap();
        match *state {
            SignalState::Notified => {}
            SignalState::Empty => *state = SignalState::Notified,
            SignalState::Waiting => {
                *state = SignalState::Empty;
                self.cond.notify_one();
            }
        }
    }
}

fn block_on_inner<F>(fut: &mut F, timeout: Timeout) -> Option<F::Output>
where
    F: Future + DropWakers,
{
    unsafe fn vtable_clone(data: *const ()) -> RawWaker {
        RawWaker::new(data, VTABLE)
    }

    unsafe fn vtable_wake(data: *const ()) {
        (&*(data as *const Signal)).notify();
    }

    unsafe fn vtable_drop(_: *const ()) {}

    const VTABLE: &'static RawWakerVTable =
        &RawWakerVTable::new(vtable_clone, vtable_wake, vtable_wake, vtable_drop);

    unsafe {
        let mut pin = std::pin::Pin::new_unchecked(fut);
        let signal = Signal::new();
        let data = &signal as *const Signal as *const ();
        let waker = Waker::from_raw(vtable_clone(data));
        let mut cx = Context::from_waker(&waker);
        let retval = loop {
            match pin.as_mut().poll(&mut cx) {
                Poll::Pending => if signal.wait(&timeout) { break None },
                Poll::Ready(item) => break Some(item),
            }
        };
        pin.get_unchecked_mut().drop_wakers();
        retval
    }
}

/// block the thread until the future is ready
pub fn block_on<F>(fut: &mut F) -> F::Output
where
    F: Future + DropWakers,
{
    block_on_inner(fut, Timeout::None).unwrap()
}

/// block the thread until the future is ready or the timeout elapses
pub fn block_on_timeout<F>(fut: &mut F, timeout: Duration) -> Option<F::Output>
where
    F: Future + DropWakers,
{
    block_on_inner(fut, Timeout::After(timeout))
}

/// poll the future and return its value if it resolves without ever blocking
pub fn dont_block_on<F: Future>(fut: &mut F) -> Option<F::Output>
where
    F: Future + DropWakers,
{
    block_on_inner(fut, Timeout::Immediate)
}
