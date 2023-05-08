use crate::async_buffer::AsyncBuffer;
use std::fmt::Display;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread::JoinHandle;
use wgpu::{BufferDescriptor, Device, Maintain};

mod sealed {
    pub(super) struct InternalAllocation;
}

/// A marker object which can only be obtained by starting an allocation scope. An instance
/// of this type is used as proof that the following allocations are being performed within
/// a scope that is checking for Out of Memory errors.
pub struct AllocationScope(sealed::InternalAllocation);

#[derive(Debug)]
pub struct OutOfMemoryError {
    pub source: Box<dyn std::error::Error + Send>,
}

impl Display for OutOfMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}

impl std::error::Error for OutOfMemoryError {}

/// Polls the device whilever a future says there is something to poll
#[derive(Debug)]
pub(crate) struct PollLoop {
    has_work: Arc<AtomicBool>,
    is_done: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl PollLoop {
    fn new(device: Arc<Device>) -> Self {
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
                        while !device.poll(Maintain::Wait) {}
                    }

                    std::thread::park();
                }
            }),
        }
    }

    pub fn start_polling(&self) {
        // On the web, we don't poll, so don't do anything
        #[cfg(target_arch = "wasm32")]
        return;

        self.has_work.store(true, Ordering::Release);
        self.handle.thread().unpark()
    }
}

impl Drop for PollLoop {
    fn drop(&mut self) {
        self.is_done.store(true, Ordering::Release);
        self.handle.thread().unpark()
    }
}

#[derive(Clone, Debug)]
pub struct AsyncDevice {
    device: Arc<Device>,
    poll_loop: Arc<PollLoop>,
}

impl AsyncDevice {
    pub fn new(device: Device) -> Self {
        let device = Arc::new(device);
        Self {
            poll_loop: Arc::new(PollLoop::new(device.clone())),
            device,
        }
    }

    pub fn do_async<F, R>(&self, f: F) -> WgpuFuture<R>
    where
        F: FnOnce(Box<dyn FnOnce(R) + Send>),
        R: Send + 'static,
    {
        let future = WgpuFuture::new(self.device.clone(), self.poll_loop.clone());
        f(future.callback());
        future
    }

    /// Runs a closure with validation if we are a debug build, or without validation if we aren't
    #[allow(unused_variables)]
    pub fn with_debug_validation<R>(&self, f: impl FnOnce() -> R, debug_str: &str) -> R {
        #[cfg(not(debug_assertions))]
        return f();

        #[cfg(debug_assertions)]
        pollster::block_on(self.with_debug_validation_async(async { f() }, debug_str))
    }

    /// Runs an async closure with validation if we are a debug build, or without validation if we aren't
    pub async fn with_debug_validation_async<R>(
        &self,
        f: impl Future<Output = R>,
        debug_str: &str,
    ) -> R {
        #[cfg(debug_assertions)]
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);

        let ret = f.await;

        #[cfg(debug_assertions)]
        if let Some(err) = self.device.pop_error_scope().await {
            panic!("Validation error on {}: {}", debug_str, err)
        }

        return ret;
    }

    /// Creates a new buffer, checking for out of memory errors, and validation errors on Debug builds.
    /// Note that checking OoM takes some non-zero time, so when allocating multiple buffers in a row
    /// prefer [`AsyncDevice::create_scoped_buffer`]
    pub async fn create_buffer<'a>(
        &self,
        desc: &BufferDescriptor<'a>,
    ) -> Result<AsyncBuffer, OutOfMemoryError> {
        self.with_allocation_scope(|scope| self.create_buffer_scoped(scope, desc))
            .await
    }

    /// Begins an allocation scope checking for out of memory errors, and validation errors on Debug builds.
    /// Used in conjunction with [`AsyncDevice::create_scoped_buffer`] - see there for more details.
    pub async fn with_allocation_scope<'a, Ret>(
        &self,
        gen: impl FnOnce(&AllocationScope) -> Ret,
    ) -> Result<Ret, OutOfMemoryError> {
        self.with_debug_validation_async(
            async {
                self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

                let scope = AllocationScope(sealed::InternalAllocation);
                let res = gen(&scope);

                // Pop OOM first
                if let Some(e) = self.device.pop_error_scope().await {
                    match e {
                        wgpu::Error::OutOfMemory { source } => Err(OutOfMemoryError { source }),
                        wgpu::Error::Validation {
                            source: _,
                            description: _,
                        } => unreachable!(),
                    }
                } else {
                    Ok(res)
                }
            },
            "scoped_allocation",
        )
        .await
    }

    /// An alternative to [`AsyncDevice::create_buffer`] that is still safe, but is also more performant, at the expense of
    /// more complicated code.
    ///
    /// # Usage
    ///
    /// First create an allocation scope, and then within that scope allocate several buffers:
    ///
    /// ```
    /// # use wgpu_async::wrap_wgpu;
    /// # use wgpu::BufferDescriptor;
    /// # let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    /// # let adapter = pollster::block_on(instance
    /// #     .request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
    /// # let (device, queue) = pollster::block_on(adapter
    /// #     .request_device(
    /// #         &wgpu::DeviceDescriptor::default(),
    /// #         None,
    /// #     ))
    /// #     .unwrap();
    /// let (device, queue) = wrap_wgpu(device, queue);
    /// let (buffer1, buffer2) =
    /// # pollster::block_on(async {
    ///     device.with_scope(|scope| {
    ///         let buffer1 = device.create_scoped_buffer(scope, &wgpu::BufferDescriptor {
    ///             label: None,
    ///             size: 1024,
    ///             usage: wgpu::BufferUsages::STORAGE,
    ///             mapped_at_creation: false,
    ///         });
    ///         let buffer2 = device.create_scoped_buffer(scope, &BufferDescriptor {
    ///             label: None,
    ///             size: 64,
    ///             usage: wgpu::BufferUsages::UNIFORM,
    ///             mapped_at_creation: false,
    ///         });
    ///     
    ///         (buffer1, buffer2)
    ///     }).await.unwrap()
    /// # });
    /// ```
    pub fn create_buffer_scoped<'a>(
        &self,
        // Proof that we are in an allocation scope that is checking OoM errors
        _scope: &AllocationScope,
        desc: &BufferDescriptor<'a>,
    ) -> AsyncBuffer {
        let buffer = self.device.create_buffer(desc);
        AsyncBuffer {
            label: desc.label.map(str::to_owned),
            device: self.clone(),
            buffer,
        }
    }

    /// Shorthand for [`AsyncDevice::create_buffer`] that can't fail and that doesn't need
    /// to be awaited, since an empty allocation can't fail and takes no time.
    pub fn create_empty_buffer(
        &self,
        usage: wgpu::BufferUsages,
        label: Option<&str>,
    ) -> AsyncBuffer {
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label,
            size: 0,
            usage,
            mapped_at_creation: false,
        });
        AsyncBuffer {
            label: label.map(str::to_owned),
            device: self.clone(),
            buffer,
        }
    }
}

// We're a smart pointer. Let everyone access our inner device.
impl Deref for AsyncDevice {
    type Target = wgpu::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
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

        // If we're not ready, make sure the poll loop is running
        self.poll_loop.start_polling();

        return Poll::Pending;
    }
}
