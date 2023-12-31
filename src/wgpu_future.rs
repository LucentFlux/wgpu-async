use std::future::Future;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll, Waker};
use wgpu::Maintain;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::AsyncDevice;

/// Polls the device whilever a future says there is something to poll.
///
/// This objects corresponds to a thread that parks itself when no futures are
/// waiting on it, and then calls `device.poll(Maintain::Wait)` repeatedly to block
/// whilever it has work that a future is waiting on.
///
/// The thread dies when this object is dropped, and when the GPU has finished processing
/// all active futures.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub(crate) struct PollLoop {
    /// The number of futures still waiting on resolution from the GPU.
    /// When this is 0, the thread can park itself.
    has_work: Arc<AtomicUsize>,
    is_done: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PollLoop {
    pub(crate) fn new(device: Weak<wgpu::Device>) -> Self {
        let has_work = Arc::new(AtomicUsize::new(0));
        let is_done = Arc::new(AtomicBool::new(false));
        let locally_has_work = has_work.clone();
        let locally_is_done = is_done.clone();
        Self {
            has_work,
            is_done,
            handle: std::thread::spawn(move || {
                while !locally_is_done.load(Ordering::Acquire) {
                    while locally_has_work.load(Ordering::Acquire) != 0 {
                        match device.upgrade() {
                            None => return, // If all other references to the device are dropped, don't keep hold of the device here
                            Some(device) => device.poll(Maintain::Wait),
                        };
                    }

                    std::thread::park();
                }
            }),
        }
    }

    /// If the loop wasn't polling, start it polling.
    fn start_polling(&self) -> PollToken {
        let prev = self.has_work.fetch_add(1, Ordering::AcqRel);
        debug_assert!(
            prev < usize::MAX,
            "cannot have more than `usize::MAX` outstanding operations on the GPU"
        );
        self.handle.thread().unpark();
        PollToken {
            work_count: Arc::clone(&self.has_work),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for PollLoop {
    fn drop(&mut self) {
        self.is_done.store(true, Ordering::Release);
        self.handle.thread().unpark()
    }
}

/// RAII indicating that polling is occurring, while this token is held.
#[cfg(not(target_arch = "wasm32"))]
struct PollToken {
    work_count: Arc<AtomicUsize>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for PollToken {
    fn drop(&mut self) {
        // On the web we don't poll, so don't do anything
        #[cfg(not(target_arch = "wasm32"))]
        {
            let prev = self.work_count.fetch_sub(1, Ordering::AcqRel);
            debug_assert!(
                prev > 0,
                "stop_polling was called without calling start_polling"
            );
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
    device: AsyncDevice,
    state: Arc<Mutex<WgpuFutureSharedState<T>>>,

    #[cfg(not(target_arch = "wasm32"))]
    poll_token: Option<PollToken>,
}

impl<T: Send + 'static> WgpuFuture<T> {
    pub(crate) fn new(device: AsyncDevice) -> Self {
        Self {
            device,
            state: Arc::new(Mutex::new(WgpuFutureSharedState {
                result: None,
                waker: None,
            })),

            #[cfg(not(target_arch = "wasm32"))]
            poll_token: None,
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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll whenever we enter to see if we can avoid waiting altogether
        self.device.poll(Maintain::Poll);

        // Check with scoped lock
        {
            let Self {
                state,
                #[cfg(not(target_arch = "wasm32"))]
                poll_token,
                ..
            } = self.as_mut().get_mut();
            let mut lock = state.lock().expect("wgpu future was poisoned on poll");

            if let Some(res) = lock.result.take() {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    // Drop token, stopping poll loop.
                    *poll_token = None;
                }

                return Poll::Ready(res);
            }

            lock.waker = Some(cx.waker().clone());
        }

        // If we're not ready, make sure the poll loop is running (on non-WASM)
        #[cfg(not(target_arch = "wasm32"))]
        if self.poll_token.is_none() {
            self.poll_token = Some(self.device.poll_loop.start_polling());
        }

        return Poll::Pending;
    }
}
