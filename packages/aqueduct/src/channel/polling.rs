// internal future polling system for channel.
//
// design based on pollster crate, but with extensive modifications.

use std::{
    future::Future,
    sync::{Condvar, Mutex},
    task::{Context, Poll, Waker, RawWaker, RawWakerVTable},
    time::Instant,
    pin::Pin,
};


// `Future` with the ability to drop all wakers it previously cloned.
//
// - this future must guarantee that if a call to Future::poll either:
//
//   - returns Poll::Ready
//   - panics
//
//   then all wakers previously cloned from calls to poll have been dropped by the time the call
//   to poll returns (or returns via panic). otherwise, undefined behavior may occur.
//
// - this future must guarantee that if drop_wakers is called, then all wakers previously cloned
//   from calls to poll have been dropped by the time the call to drop_wakers returns (or returns
//   via panic). otherwise, undefined behavior may occur.
//
// it is expected that drop_wakers will be called only if the future has not yet resolved, and that
// the future will never again be polled after drop_wakers is called. the future may panic or
// exhibit bugs if this assumption is violated. however, the future must not trigger undefined
// behavior if this assumption is violated, including by violating the above requirements.
pub(crate) unsafe trait DropWakers: Future {
    type DropWakersOutput;

    fn drop_wakers(&mut self) -> Self::DropWakersOutput;
}

// timeout for blocking on a future.
pub(crate) enum Timeout {
    // never time out.
    Never,
    // time out at the given deadline.
    At(Instant),
    // time out if the future is cannot be resolved without blocking.
    NonBlocking,
}

// poll the future until it resolves or the timeout is reached, in which case return err.
pub(crate) fn poll<F>(fut: &mut F, timeout: Timeout) -> Result<F::Output, F::DropWakersOutput>
where
    F: Future + DropWakers + Unpin,
{
    unsafe {
        // our Waker's data pointer is just to this Signal local variable. the DropWakers unsafe
        // trait allows us to ensure all references to it are eliminated before we return.
        let signal = Signal {
            state: Mutex::new(State::Empty),
            cond: Condvar::new(),
        };

        // construct context and poll
        let data = &signal as *const Signal as *const ();
        let waker = Waker::from_raw(vtable_clone(data));
        let mut cx = Context::from_waker(&waker);
        // safety: poll_inner will call fut.poll, which may panic. however, fut unsafe-implements
        //         DropWakers, so it must drop any cloned wakers before panicking.
        let resolved = poll_inner(fut, &signal, &mut cx, timeout);

        // clean up before returning.
        // safety: drop_wakers may panic. however, DropWakers requires that if drop_wakers does
        //         exit via panic, it drops any cloned wakers before doing so, so there will be no
        //         dangling poiinters to the signal local variable as unwinding occurs. 
        let to_return = resolved.ok_or_else(|| fut.drop_wakers());
        // safety: fut unsafe-implements DropWakers, so:
        //
        // - if it returned Ready, it must have already dropped any cloned wakers.
        // - if it didn't return Ready, we called drop_wakers, in which case it must have already
        //   dropped any cloned wakers.
        //
        // therefore, at this point, there are no dangling pointers to the signal local variable,
        // so it can be dropped.
        drop(signal);

        // done
        to_return
    }
}

// poll future with context until resolves or times out.
unsafe fn poll_inner<F>(
    fut: &mut F,
    signal: &Signal,
    cx: &mut Context,
    timeout: Timeout,
) -> Option<F::Output>
where
    F: Future + DropWakers + Unpin,
{
    loop {
        // return if ready
        if let Poll::Ready(output) = Pin::new(&mut *fut).as_mut().poll(cx) {
            return Some(output);
        }

        // otherwise, block until notification or timeout
        let mut lock = signal.state.lock().unwrap();

        // if a notification is already present, skip to the next loop iteration so as to release
        // the lock and try polling again without blocking.
        if let &State::Notified = &*lock {
            *lock = State::Empty;
            continue;
        }

        // otherwise, actually block until notification or timeout
        debug_assert!(matches!(&*lock, State::Empty));
        *lock = State::Waiting;
        match &timeout {
            // block on mutex + condvar indefinitely
            &Timeout::Never =>
                while let &State::Waiting = &*lock {
                    lock = signal.cond.wait(lock).unwrap();
                },

            // block on mutex + condvar until deadline, at which point return none
            &Timeout::At(deadline) =>
                while let &State::Waiting = &*lock {
                    let Some(duration) =
                        deadline.checked_duration_since(Instant::now())
                        else { return None };
                    let (lock2, wait_result) = signal.cond.wait_timeout(lock, duration).unwrap();
                    lock = lock2;
                    if wait_result.timed_out() { return None; }
                },

            // dont block on mutex + condvar, return none instead
            &Timeout::NonBlocking => return None,
        }
        *lock = State::Empty;
    }
}

// synchronization signal state
enum State {
    Empty,
    Waiting,
    Notified,
}

// synchronization signal
struct Signal {
    state: Mutex<State>,
    cond: Condvar,
}


// ==== vtable ====

const VTABLE: &'static RawWakerVTable =
    &RawWakerVTable::new(vtable_clone, vtable_wake, vtable_wake, vtable_drop);

unsafe fn vtable_clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, VTABLE)
}

unsafe fn vtable_wake(data: *const ()) {
    // notify signal
    let signal = &*(data as *const Signal);
    let mut lock = signal.state.lock().unwrap();
    match &*lock {
        &State::Notified => (),
        &State::Empty => {
            *lock = State::Notified;
        }
        &State::Waiting => {
            *lock = State::Empty;
            signal.cond.notify_one();
        }
    }
}

unsafe fn vtable_drop(_data: *const ()) {}
