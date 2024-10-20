//! Futures and other types that allow asynchronous interaction with channels.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    sync::Mutex,
    any::Any,
    ops::Deref,
};
use std::fmt::{Debug, Formatter};
use super::*;

struct AsyncSignal {
    waker: Mutex<Waker>,
    woken: AtomicBool,
    stream: bool,
}

impl AsyncSignal {
    fn new(cx: &Context, stream: bool) -> Self {
        AsyncSignal {
            waker: Mutex::new(cx.waker().clone()),
            woken: AtomicBool::new(false),
            stream,
        }
    }
}

impl Signal for AsyncSignal {
    fn fire(&self) -> bool {
        self.woken.store(true, Ordering::SeqCst);
        self.waker.lock().wake_by_ref();
        self.stream
    }

    fn as_any(&self) -> &(dyn Any + 'static) { self }
    fn as_ptr(&self) -> *const () { self as *const _ as *const () }
}

impl<T> Hook<T, AsyncSignal> {
    // Update the hook to point to the given Waker.
    // Returns whether the hook has been previously awakened
    fn update_waker(&self, cx_waker: &Waker) -> bool {
        let mut waker = self.1.waker.lock();
        let woken = self.1.woken.load(Ordering::SeqCst);
        if !waker.will_wake(cx_waker) {
            *waker = cx_waker.clone();

            // Avoid the edge case where the waker was woken just before the wakers were
            // swapped.
            if woken {
                cx_waker.wake_by_ref();
            }
        }
        woken
    }
}

#[derive(Clone)]
enum OwnedOrRef<'a, T> {
    Owned(T),
    Ref(&'a T),
}

impl<'a, T> Deref for OwnedOrRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            OwnedOrRef::Owned(arc) => &arc,
            OwnedOrRef::Ref(r) => r,
        }
    }
}

impl<T> Sender<T> {
    /// Asynchronously send a value into the channel, returning an error if all receivers have been
    /// dropped. If the channel is bounded and is full, the returned future will yield to the async
    /// runtime.
    ///
    /// In the current implementation, the returned future will not yield to the async runtime if the
    /// channel is unbounded. This may change in later versions.
    pub fn send_async(&self, item: T) -> SendFut<T> {
        SendFut {
            sender: OwnedOrRef::Ref(&self),
            hook: Some(SendState::NotYetSent(item)),
        }
    }
}

enum SendState<T> {
    NotYetSent(T),
    QueuedItem(Arc<Hook<T, AsyncSignal>>),
}

/// A future that sends a value into a channel.
///
/// Can be created via [`Sender::send_async`] or [`Sender::into_send_async`].
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct SendFut<'a, T> {
    sender: OwnedOrRef<'a, Sender<T>>,
    // Only none after dropping
    hook: Option<SendState<T>>,
}

impl<T> std::marker::Unpin for SendFut<'_, T> {}

impl<'a, T> SendFut<'a, T> {
    /// Reset the hook, clearing it and removing it from the waiting sender's queue. This is called
    /// on drop and just before `start_send` in the `Sink` implementation.
    fn reset_hook(&mut self) {
        if let Some(SendState::QueuedItem(hook)) = self.hook.take() {
            let hook: Arc<Hook<T, dyn Signal>> = hook;
            self.sender.shared.chan.lock().unwrap().sending
                .as_mut()
                .unwrap().1
                .retain(|s| s.signal().as_ptr() != hook.signal().as_ptr());
        }
    }

    /// See [`Sender::is_disconnected`].
    pub fn is_disconnected(&self) -> bool {
        self.sender.is_disconnected()
    }

    /// See [`Sender::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }

    /// See [`Sender::is_full`].
    pub fn is_full(&self) -> bool {
        self.sender.is_full()
    }

    /// See [`Sender::len`].
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    /// See [`Sender::capacity`].
    pub fn capacity(&self) -> Option<usize> {
        self.sender.capacity()
    }
}

impl<'a, T> Drop for SendFut<'a, T> {
    fn drop(&mut self) {
        self.reset_hook()
    }
}


impl<'a, T> Future for SendFut<'a, T> {
    type Output = Result<(), SendError<T>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(SendState::QueuedItem(hook)) = self.hook.as_ref() {
            if hook.is_empty() {
                Poll::Ready(Ok(()))
            } else if self.sender.shared.is_disconnected() {
                let item = hook.try_take();
                self.hook = None;
                match item {
                    Some(item) => Poll::Ready(Err(SendError(item))),
                    None => Poll::Ready(Ok(())),
                }
            } else {
                hook.update_waker(cx.waker());
                Poll::Pending
            }
        } else if let Some(SendState::NotYetSent(item)) = self.hook.take() {
            let this = self.get_mut();
            let (shared, this_hook) = (&this.sender.shared, &mut this.hook);

            shared.send(
                // item
                item,
                // should_block
                true,
                // make_signal
                |msg| Hook::slot(Some(msg), AsyncSignal::new(cx, false)),
                // do_block
                |hook| {
                    *this_hook = Some(SendState::QueuedItem(hook));
                    Poll::Pending
                },
            )
                .map(|r| r.map_err(|err| match err {
                    TrySendTimeoutError::Disconnected(msg) => SendError(msg),
                    _ => unreachable!(),
                }))
        } else { // Nothing to do
            Poll::Ready(Ok(()))
        }
    }
}

impl<T> Receiver<T> {
    /// Asynchronously receive a value from the channel, returning an error if all senders have been
    /// dropped. If the channel is empty, the returned future will yield to the async runtime.
    pub fn recv_async(&self) -> RecvFut<'_, T> {
        RecvFut::new(OwnedOrRef::Ref(self))
    }

    /// Convert this receiver into a future that asynchronously receives a single message from the
    /// channel, returning an error if all senders have been dropped. If the channel is empty, this
    /// future will yield to the async runtime.
    pub fn into_recv_async<'a>(self) -> RecvFut<'a, T> {
        RecvFut::new(OwnedOrRef::Owned(self))
    }

    /// Create an asynchronous stream that uses this receiver to asynchronously receive messages
    /// from the channel. The receiver will continue to be usable after the stream has been dropped.
    pub fn stream(&self) -> RecvStream<'_, T> {
        RecvStream(RecvFut::new(OwnedOrRef::Ref(self)))
    }

    /// Convert this receiver into a stream that allows asynchronously receiving messages from the channel.
    pub fn into_stream<'a>(self) -> RecvStream<'a, T> {
        RecvStream(RecvFut::new(OwnedOrRef::Owned(self)))
    }
}

/// A future which allows asynchronously receiving a message.
///
/// Can be created via [`Receiver::recv_async`] or [`Receiver::into_recv_async`].
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct RecvFut<'a, T> {
    receiver: OwnedOrRef<'a, Receiver<T>>,
    hook: Option<Arc<Hook<T, AsyncSignal>>>,
}

impl<'a, T> RecvFut<'a, T> {
    fn new(receiver: OwnedOrRef<'a, Receiver<T>>) -> Self {
        Self {
            receiver,
            hook: None,
        }
    }

    /// Reset the hook, clearing it and removing it from the waiting receivers queue and waking
    /// another receiver if this receiver has been woken, so as not to cause any missed wakeups.
    /// This is called on drop and after a new item is received in `Stream::poll_next`.
    fn reset_hook(&mut self) {
        if let Some(hook) = self.hook.take() {
            let hook: Arc<Hook<T, dyn Signal>> = hook;
            let mut chan = self.receiver.shared.chan.lock().unwrap();
            // We'd like to use `Arc::ptr_eq` here but it doesn't seem to work consistently with wide pointers?
            chan.waiting.retain(|s| s.signal().as_ptr() != hook.signal().as_ptr());
            if hook.signal().as_any().downcast_ref::<AsyncSignal>().unwrap().woken.load(Ordering::SeqCst) {
                // If this signal has been fired, but we're being dropped (and so not listening to it),
                // pass the signal on to another receiver
                chan.try_wake_receiver_if_pending();
            }
        }
    }

    fn poll_inner(
        self: Pin<&mut Self>,
        cx: &mut Context,
        stream: bool,
    ) -> Poll<Result<T, RecvError>> {
        if self.hook.is_some() {
            match self.receiver.shared.recv_sync(None) {
                Ok(msg) => return Poll::Ready(Ok(msg)),
                Err(TryRecvTimeoutError::Disconnected) => {
                    return Poll::Ready(Err(RecvError::Disconnected))
                }
                _ => (),
            }

            let hook = self.hook.as_ref().map(Arc::clone).unwrap();
            if hook.update_waker(cx.waker()) {
                // If the previous hook was awakened, we need to insert it back to the
                // queue, otherwise, it remains valid.
                self.receiver.shared.chan.lock().unwrap()
                    .waiting
                    .push_back(hook);
            }
            // To avoid a missed wakeup, re-check disconnect status here because the channel might have
            // gotten shut down before we had a chance to push our hook
            if self.receiver.shared.is_disconnected() {
                // And now, to avoid a race condition between the first recv attempt and the disconnect check we
                // just performed, attempt to recv again just in case we missed something.
                Poll::Ready(
                    self.receiver
                        .shared
                        .recv_sync(None)
                        .map(Ok)
                        .unwrap_or(Err(RecvError::Disconnected)),
                )
            } else {
                Poll::Pending
            }
        } else {
            let mut_self = self.get_mut();
            let (shared, this_hook) = (&mut_self.receiver.shared, &mut mut_self.hook);

            shared.recv(
                // should_block
                true,
                // make_signal
                || Hook::trigger(AsyncSignal::new(cx, stream)),
                // do_block
                |hook| {
                    *this_hook = Some(hook);
                    Poll::Pending
                },
            )
                .map(|r| r.map_err(|err| match err {
                    TryRecvTimeoutError::Disconnected => RecvError::Disconnected,
                    _ => unreachable!(),
                }))
        }
    }

    /// See [`Receiver::is_disconnected`].
    pub fn is_disconnected(&self) -> bool {
        self.receiver.is_disconnected()
    }

    /// See [`Receiver::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }

    /// See [`Receiver::is_full`].
    pub fn is_full(&self) -> bool {
        self.receiver.is_full()
    }

    /// See [`Receiver::len`].
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    /// See [`Receiver::capacity`].
    pub fn capacity(&self) -> Option<usize> {
        self.receiver.capacity()
    }
}

impl<'a, T> Debug for RecvFut<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecvFut").finish()
    }
}

impl<'a, T> Drop for RecvFut<'a, T> {
    fn drop(&mut self) {
        self.reset_hook();
    }
}

impl<'a, T> Future for RecvFut<'a, T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_inner(cx, false) // stream = false
    }
}
