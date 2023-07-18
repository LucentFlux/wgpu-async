use std::future::Future;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use wgpu::{Device, Maintain};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};

/// Polls the device whilever a future says there is something to poll.
///
/// This objects corresponds to a thread that parks itself when no futures are
/// waiting on it, and then calls `device.poll(Maintain::Wait)` repeatedly to block
/// whilever it has work that a future is waiting on.
///
/// The thread dies when this object is dropped, and when the GPU has finished processing
/// all active futures.
#[derive(Debug)]
pub(crate) struct PollLoop {
    #[cfg(not(target_arch = "wasm32"))]
    has_work: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    is_done: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    handle: std::thread::JoinHandle<()>,
}

impl PollLoop {
    pub(crate) fn new(device: Arc<Device>) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = device;
            Self {}
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let has_work = Arc::new(AtomicBool::new(false));
            let is_done = Arc::new(AtomicBool::new(false));
            let locally_has_work = has_work.clone();
            let locally_is_done = is_done.clone();
            Self {
                has_work,
                is_done,
                handle: std::thread::spawn(move || {
                    while !locally_is_done.load(Ordering::Acquire) {
                        while locally_has_work.swap(false, Ordering::AcqRel) {
                            device.poll(Maintain::Wait);
                        }

                        std::thread::park();
                    }
                }),
            }
        }
    }

    pub(crate) fn start_polling(&self) {
        // On the web we don't poll, so don't do anything
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.has_work.store(true, Ordering::Release);
            self.handle.thread().unpark()
        }
    }
}

impl Drop for PollLoop {
    fn drop(&mut self) {
        // On the web we don't poll, so don't do anything
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.is_done.store(true, Ordering::Release);
            self.handle.thread().unpark()
        }
    }
}

/// The state that both the future and the callback hold.
struct WgpuFutureSharedState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

/// A future that can be awaited for once a callback completes. Created using [`AsyncDevice::do_async`].
pub struct WgpuFuture<T> {
    device: Arc<Device>,
    poll_loop: Arc<PollLoop>,
    state: Arc<Mutex<WgpuFutureSharedState<T>>>,
}

impl<T: Send + 'static> WgpuFuture<T> {
    pub(crate) fn new(device: Arc<Device>, poll_loop: Arc<PollLoop>) -> Self {
        Self {
            device,
            poll_loop,
            state: Arc::new(Mutex::new(WgpuFutureSharedState {
                result: None,
                waker: None,
            })),
        }
    }

    /// Generates a callback function for this future that wakes the waker and sets the shared state.
    pub(crate) fn callback(&self) -> Box<dyn FnOnce(T) + Send> {
        let shared_state = self.state.clone();
        return Box::new(move |res: T| {
            let mut lock = shared_state
                .lock()
                .expect("wgpu future was poisoned on complete");
            let shared_state = lock.deref_mut();
            shared_state.result = Some(res);

            if let Some(waker) = shared_state.waker.take() {
                waker.wake()
            }
        });
    }
}

impl<T> Future for WgpuFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll whenever we enter to see if we can avoid waiting altogether
        self.device.poll(Maintain::Poll);

        // Check with scoped lock
        {
            let mut lock = self.state.lock().expect("wgpu future was poisoned on poll");

            if let Some(res) = lock.result.take() {
                return Poll::Ready(res);
            }

            lock.waker = Some(cx.waker().clone());
        }

        // If we're not ready, make sure the poll loop is running
        self.poll_loop.start_polling();

        return Poll::Pending;
    }
}
