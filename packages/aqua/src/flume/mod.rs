// heavily modified fork of the flume crate

pub mod select;
pub mod r#async;

mod signal;

pub use select::Selector;

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, AtomicBool, Ordering},
        Arc,
        Weak,
        Mutex,
        MutexGuard,
    },
    time::{Duration, Instant},
    marker::PhantomData,
    thread,
};
use crate::signal::{Signal, SyncSignal};

pub struct SendError<T>(pub T);

pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

pub enum SendTimeoutError<T> {
    Timeout(T),
    Disconnected(T),
}

enum TrySendTimeoutError<T> {
    Full(T),
    Disconnected(T),
    Timeout(T),
}

pub struct RecvError;

pub enum TryRecvError {
    Empty,
    Disconnected,
}

pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

enum TryRecvTimeoutError {
    Empty,
    Timeout,
    Disconnected,
}

struct Hook<T, S: ?Sized>(Option<Mutex<Option<T>>>, S);

impl<T, S: ?Sized + Signal> Hook<T, S> {
    pub fn slot(msg: Option<T>, signal: S) -> Arc<Self>
    where
        S: Sized,
    {
        Arc::new(Self(Some(Mutex::new(msg)), signal))
    }

    fn lock(&self) -> Option<MutexGuard<'_, Option<T>>> {
        self.0.as_ref().map(|s| s.lock().unwrap())
    }
}

impl<T, S: ?Sized + Signal> Hook<T, S> {
    pub fn fire_recv(&self) -> (T, &S) {
        let msg = self.lock().unwrap().take().unwrap();
        (msg, self.signal())
    }

    pub fn fire_send(&self, msg: T) -> (Option<T>, &S) {
        let ret = match self.lock() {
            Some(mut lock) => {
                *lock = Some(msg);
                None
            }
            None => Some(msg),
        };
        (ret, self.signal())
    }

    pub fn is_empty(&self) -> bool {
        self.lock().map(|s| s.is_none()).unwrap_or(true)
    }

    pub fn try_take(&self) -> Option<T> {
        self.lock().unwrap().take()
    }

    pub fn trigger(signal: S) -> Arc<Self>
    where
        S: Sized,
    {
        Arc::new(Self(None, signal))
    }

    pub fn signal(&self) -> &S {
        &self.1
    }

    pub fn fire_nothing(&self) -> bool {
        self.signal().fire()
    }
}

impl<T> Hook<T, SyncSignal> {
    pub fn wait_recv(&self, abort: &AtomicBool) -> Option<T> {
        loop {
            let disconnected = abort.load(Ordering::SeqCst); // Check disconnect *before* msg
            let msg = self.lock().unwrap().take();
            if let Some(msg) = msg {
                break Some(msg);
            } else if disconnected {
                break None;
            } else {
                self.signal().wait()
            }
        }
    }

    // Err(true) if timeout
    pub fn wait_deadline_recv(&self, abort: &AtomicBool, deadline: Instant) -> Result<T, bool> {
        loop {
            let disconnected = abort.load(Ordering::SeqCst); // Check disconnect *before* msg
            let msg = self.lock().unwrap().take();
            if let Some(msg) = msg {
                break Ok(msg);
            } else if disconnected {
                break Err(false);
            } else if let Some(dur) = deadline.checked_duration_since(Instant::now()) {
                self.signal().wait_timeout(dur);
            } else {
                break Err(true);
            }
        }
    }

    pub fn wait_send(&self, abort: &AtomicBool) {
        loop {
            let disconnected = abort.load(Ordering::SeqCst); // Check disconnect *before* msg
            if disconnected || self.lock().unwrap().is_none() {
                break;
            }

            self.signal().wait();
        }
    }

    // Err(true) if timeout
    pub fn wait_deadline_send(&self, abort: &AtomicBool, deadline: Instant) -> Result<(), bool> {
        loop {
            let disconnected = abort.load(Ordering::SeqCst); // Check disconnect *before* msg
            if self.lock().unwrap().is_none() {
                break Ok(());
            } else if disconnected {
                break Err(false);
            } else if let Some(dur) = deadline.checked_duration_since(Instant::now()) {
                self.signal().wait_timeout(dur);
            } else {
                break Err(true);
            }
        }
    }
}


struct Chan<T> {
    sending: Option<(usize, VecDeque<Arc<Hook<T, dyn signal::Signal>>>)>,
    queue: VecDeque<T>,
    waiting: VecDeque<Arc<Hook<T, dyn signal::Signal>>>,
}

impl<T> Chan<T> {
    fn pull_pending(&mut self, pull_extra: bool) {
        if let Some((cap, sending)) = &mut self.sending {
            let effective_cap = *cap + pull_extra as usize;

            while self.queue.len() < effective_cap {
                if let Some(s) = sending.pop_front() {
                    let (msg, signal) = s.fire_recv();
                    signal.fire();
                    self.queue.push_back(msg);
                } else {
                    break;
                }
            }
        }
    }

    fn try_wake_receiver_if_pending(&mut self) {
        if !self.queue.is_empty() {
            while Some(false) == self.waiting.pop_front().map(|s| s.fire_nothing()) {}
        }
    }
}

struct Shared<T> {
    chan: Mutex<Chan<T>>,
    disconnected: AtomicBool,
    sender_count: AtomicUsize,
    receiver_count: AtomicUsize,
}

impl<T> Shared<T> {
    fn new(cap: Option<usize>) -> Self {
        Self {
            chan: Mutex::new(Chan {
                sending: cap.map(|cap| (cap, VecDeque::new())),
                queue: VecDeque::new(),
                waiting: VecDeque::new(),
            }),
            disconnected: AtomicBool::new(false),
            sender_count: AtomicUsize::new(1),
            receiver_count: AtomicUsize::new(1),
        }
    }

    fn send<S: Signal, R: From<Result<(), TrySendTimeoutError<T>>>>(
        &self,
        msg: T,
        should_block: bool,
        make_signal: impl FnOnce(T) -> Arc<Hook<T, S>>,
        do_block: impl FnOnce(Arc<Hook<T, S>>) -> R,
    ) -> R {
        let mut chan = self.chan.lock().unwrap();

        if self.is_disconnected() {
            Err(TrySendTimeoutError::Disconnected(msg)).into()
        } else if !chan.waiting.is_empty() {
            let mut msg = Some(msg);

            loop {
                let slot = chan.waiting.pop_front();
                match slot.as_ref().map(|r| r.fire_send(msg.take().unwrap())) {
                    // No more waiting receivers and msg in queue, so break out of the loop
                    None if msg.is_none() => break,
                    // No more waiting receivers, so add msg to queue and break out of the loop
                    None => {
                        chan.queue.push_back(msg.unwrap());
                        break;
                    }
                    Some((Some(m), signal)) => {
                        if signal.fire() {
                            // Was async and a stream, so didn't acquire the message. Wake another
                            // receiver, and do not yet push the message.
                            msg.replace(m);
                            continue;
                        } else {
                            // Was async and not a stream, so it did acquire the message. Push the
                            // message to the queue for it to be received.
                            chan.queue.push_back(m);
                            drop(chan);
                            break;
                        }
                    },
                    Some((None, signal)) => {
                        drop(chan);
                        signal.fire();
                        break; // Was sync, so it has acquired the message
                    },
                }
            }

            Ok(()).into()
        } else if chan.sending.as_ref().map(|(cap, _)| chan.queue.len() < *cap).unwrap_or(true) {
            chan.queue.push_back(msg);
            Ok(()).into()
        } else if should_block { // Only bounded from here on
            let hook = make_signal(msg);
            chan.sending.as_mut().unwrap().1.push_back(hook.clone());
            drop(chan);

            do_block(hook)
        } else {
            Err(TrySendTimeoutError::Full(msg)).into()
        }
    }

    fn send_sync(
        &self,
        msg: T,
        block: Option<Option<Instant>>,
    ) -> Result<(), TrySendTimeoutError<T>> {
        self.send(
            // msg
            msg,
            // should_block
            block.is_some(),
            // make_signal
            |msg| Hook::slot(Some(msg), SyncSignal::default()),
            // do_block
            |hook| if let Some(deadline) = block.unwrap() {
                hook.wait_deadline_send(&self.disconnected, deadline)
                    .or_else(|timed_out| {
                        if timed_out { // Remove our signal
                            let hook: Arc<Hook<T, dyn signal::Signal>> = hook.clone();
                            self.chan.lock().unwrap().sending
                                .as_mut()
                                .unwrap().1
                                .retain(|s| s.signal().as_ptr() != hook.signal().as_ptr());
                        }
                        hook.try_take().map(|msg| if self.is_disconnected() {
                            Err(TrySendTimeoutError::Disconnected(msg))
                        } else {
                            Err(TrySendTimeoutError::Timeout(msg))
                        })
                        .unwrap_or(Ok(()))
                    })
            } else {
                hook.wait_send(&self.disconnected);

                match hook.try_take() {
                    Some(msg) => Err(TrySendTimeoutError::Disconnected(msg)),
                    None => Ok(()),
                }
            },
        )
    }

    fn recv<S: Signal, R: From<Result<T, TryRecvTimeoutError>>>(
        &self,
        should_block: bool,
        make_signal: impl FnOnce() -> Arc<Hook<T, S>>,
        do_block: impl FnOnce(Arc<Hook<T, S>>) -> R,
    ) -> R {
        let mut chan = self.chan.lock().unwrap();
        chan.pull_pending(true);

        if let Some(msg) = chan.queue.pop_front() {
            drop(chan);
            Ok(msg).into()
        } else if self.is_disconnected() {
            drop(chan);
            Err(TryRecvTimeoutError::Disconnected).into()
        } else if should_block {
            let hook = make_signal();
            chan.waiting.push_back(hook.clone());
            drop(chan);

            do_block(hook)
        } else {
            drop(chan);
            Err(TryRecvTimeoutError::Empty).into()
        }
    }

    fn recv_sync(&self, block: Option<Option<Instant>>) -> Result<T, TryRecvTimeoutError> {
        self.recv(
            // should_block
            block.is_some(),
            // make_signal
            || Hook::slot(None, SyncSignal::default()),
            // do_block
            |hook| if let Some(deadline) = block.unwrap() {
                hook.wait_deadline_recv(&self.disconnected, deadline)
                    .or_else(|timed_out| {
                        if timed_out { // Remove our signal
                            let hook: Arc<Hook<T, dyn Signal>> = hook.clone();
                            self.chan.lock().unwrap().waiting
                                .retain(|s| s.signal().as_ptr() != hook.signal().as_ptr());
                        }
                        match hook.try_take() {
                            Some(msg) => Ok(msg),
                            None => {
                                let disconnected = self.is_disconnected(); // Check disconnect *before* msg
                                if let Some(msg) = self.chan.lock().unwrap().queue.pop_front() {
                                    Ok(msg)
                                } else if disconnected {
                                    Err(TryRecvTimeoutError::Disconnected)
                                } else {
                                    Err(TryRecvTimeoutError::Timeout)
                                }
                            },
                        }
                    })
            } else {
                hook.wait_recv(&self.disconnected)
                    .or_else(|| self.chan.lock().unwrap().queue.pop_front())
                    .ok_or(TryRecvTimeoutError::Disconnected)
            },
        )
    }

    /// Disconnect anything listening on this channel (this will not prevent receivers receiving
    /// msgs that have already been sent)
    fn disconnect_all(&self) {
        self.disconnected.store(true, Ordering::Relaxed);

        let mut chan = self.chan.lock().unwrap();
        chan.pull_pending(false);
        if let Some((_, sending)) = chan.sending.as_ref() {
            sending.iter().for_each(|hook| {
                hook.signal().fire();
            })
        }
        chan.waiting.iter().for_each(|hook| {
            hook.signal().fire();
        });
    }

    fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::SeqCst)
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.capacity().map(|cap| cap == self.len()).unwrap_or(false)
    }

    fn len(&self) -> usize {
        let mut chan = self.chan.lock().unwrap();
        chan.pull_pending(false);
        chan.queue.len()
    }

    fn capacity(&self) -> Option<usize> {
        self.chan.lock().unwrap().sending.as_ref().map(|(cap, _)| *cap)
    }

    fn sender_count(&self) -> usize {
        self.sender_count.load(Ordering::Relaxed)
    }

    fn receiver_count(&self) -> usize {
        self.receiver_count.load(Ordering::Relaxed)
    }
}

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Sender<T> {
    pub fn try_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.shared.send_sync(msg, None).map_err(|err| match err {
            TrySendTimeoutError::Full(msg) => TrySendError::Full(msg),
            TrySendTimeoutError::Disconnected(msg) => TrySendError::Disconnected(msg),
            _ => unreachable!(),
        })
    }
    
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        self.shared.send_sync(msg, Some(None)).map_err(|err| match err {
            TrySendTimeoutError::Disconnected(msg) => SendError(msg),
            _ => unreachable!(),
        })
    }

    pub fn send_deadline(&self, msg: T, deadline: Instant) -> Result<(), SendTimeoutError<T>> {
        self.shared.send_sync(msg, Some(Some(deadline))).map_err(|err| match err {
            TrySendTimeoutError::Disconnected(msg) => SendTimeoutError::Disconnected(msg),
            TrySendTimeoutError::Timeout(msg) => SendTimeoutError::Timeout(msg),
            _ => unreachable!(),
        })
    }

    pub fn send_timeout(&self, msg: T, dur: Duration) -> Result<(), SendTimeoutError<T>> {
        self.send_deadline(msg, Instant::now().checked_add(dur).unwrap())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.sender_count.fetch_add(1, Ordering::Relaxed);
        Self { shared: self.shared.clone() }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Notify receivers that all senders have been dropped if the number of senders drops to 0.
        if self.shared.sender_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.shared.disconnect_all();
        }
    }
}

pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.shared.recv_sync(None).map_err(|err| match err {
            TryRecvTimeoutError::Disconnected => TryRecvError::Disconnected,
            TryRecvTimeoutError::Empty => TryRecvError::Empty,
            _ => unreachable!(),
        })
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        self.shared.recv_sync(Some(None)).map_err(|err| match err {
            TryRecvTimeoutError::Disconnected => RecvError::Disconnected,
            _ => unreachable!(),
        })
    }

    pub fn recv_deadline(&self, deadline: Instant) -> Result<T, RecvTimeoutError> {
        self.shared.recv_sync(Some(Some(deadline))).map_err(|err| match err {
            TryRecvTimeoutError::Disconnected => RecvTimeoutError::Disconnected,
            TryRecvTimeoutError::Timeout => RecvTimeoutError::Timeout,
            _ => unreachable!(),
        })
    }

    pub fn recv_timeout(&self, dur: Duration) -> Result<T, RecvTimeoutError> {
        self.recv_deadline(Instant::now().checked_add(dur).unwrap())
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.shared.receiver_count.fetch_add(1, Ordering::Relaxed);
        Self { shared: self.shared.clone() }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // Notify senders that all receivers have been dropped if the number of receivers drops
        // to 0.
        if self.shared.receiver_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.shared.disconnect_all();
        }
    }
}

pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(None));
    (
        Sender { shared: shared.clone() },
        Receiver { shared },
    )
}

pub fn bounded<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(Some(cap)));
    (
        Sender { shared: shared.clone() },
        Receiver { shared },
    )
}
