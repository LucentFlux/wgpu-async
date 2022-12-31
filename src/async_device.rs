use crate::async_buffer::AsyncBuffer;
use std::fmt::Display;
use std::future::Future;
use std::ops::DerefMut;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use wgpu::{BufferDescriptor, Device, Maintain};

#[derive(Debug)]
pub struct OutOfMemoryError {
    source: Box<dyn std::error::Error + Send>,
}

impl Display for OutOfMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for OutOfMemoryError {}

#[derive(Clone, Debug)]
pub struct AsyncDevice {
    device: Arc<Device>,
}

impl AsyncDevice {
    pub fn new(device: Device) -> Self {
        Self {
            device: Arc::new(device),
        }
    }

    pub fn do_async<F, R>(&self, f: F) -> WgpuFuture<R>
    where
        F: FnOnce(Box<dyn FnOnce(R) + Send>),
        R: Send + 'static,
    {
        let future = WgpuFuture::new(self.device.clone());
        f(future.callback());
        future
    }

    pub async fn create_buffer<'a>(
        &self,
        desc: &BufferDescriptor<'a>,
    ) -> Result<AsyncBuffer, OutOfMemoryError> {
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let buffer = self.device.create_buffer(desc);

        if let Some(e) = self.device.pop_error_scope().await {
            match e {
                wgpu::Error::OutOfMemory { source } => return Err(OutOfMemoryError { source }),
                wgpu::Error::Validation {
                    source: _,
                    description: _,
                } => unreachable!(),
            }
        }

        Ok(AsyncBuffer::new(self.clone(), buffer))
    }
}

impl AsRef<Device> for AsyncDevice {
    fn as_ref(&self) -> &Device {
        return self.device.as_ref();
    }
}

impl Eq for AsyncDevice {}
impl PartialEq for AsyncDevice {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.device, &other.device)
    }
}

struct WgpuFutureSharedState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

pub struct WgpuFuture<T> {
    device: Arc<Device>,
    state: Arc<Mutex<WgpuFutureSharedState<T>>>,
}

impl<T: Send + 'static> WgpuFuture<T> {
    pub fn new(device: Arc<Device>) -> Self {
        Self {
            device,
            state: Arc::new(Mutex::new(WgpuFutureSharedState {
                result: None,
                waker: None,
            })),
        }
    }

    /// Generates a callback function for this future that wakes the waker and sets the shared state
    pub fn callback(&self) -> Box<dyn FnOnce(T) + Send> {
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

        // Treat as green thread - we pass back but are happy to sit in a spin loop and poll
        cx.waker().wake_by_ref();

        return Poll::Pending;
    }
}
