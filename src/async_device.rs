use crate::AsyncBuffer;
use crate::WgpuFuture;
use std::ops::Deref;
use std::sync::Arc;
use wgpu::Device;

/// A wrapper around a [`wgpu::Device`] which shadows some methods to allow for callback-and-poll
/// methods to be made async.
#[derive(Clone, Debug)]
pub struct AsyncDevice {
    // PollLoop must be dropped before device to ensure device is not dropped on poll thread
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) poll_loop: Arc<crate::wgpu_future::PollLoop>,

    device: Arc<Device>,
}

impl AsyncDevice {
    pub(crate) fn new(device: Arc<Device>) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            poll_loop: Arc::new(crate::wgpu_future::PollLoop::new(Arc::downgrade(&device))),
            device,
        }
    }

    /// Converts a callback-and-poll `wgpu` method pair into a future.
    ///
    /// The function given is called immediately, usually initiating work on the GPU immediately, however
    /// the device is only polled once the future is awaited.
    ///
    /// # Example
    ///
    /// The `Buffer::map_async` method is made async using this method:
    ///
    /// ```
    /// # let _ = stringify! {
    /// let future = device.do_async(|callback|
    ///     buffer_slice.map_async(mode, callback)
    /// );
    /// let result = future.await;
    /// # };
    /// ```
    pub fn do_async<F, R>(&self, f: F) -> WgpuFuture<R>
    where
        F: FnOnce(Box<dyn FnOnce(R) + Send>),
        R: Send + 'static,
    {
        let future = WgpuFuture::new(self.clone());
        f(future.callback());
        future
    }

    /// Creates an [`AsyncBuffer`].
    pub fn create_buffer(&self, desc: &wgpu::BufferDescriptor) -> AsyncBuffer {
        AsyncBuffer {
            device: self.clone(),
            buffer: self.device.create_buffer(desc),
        }
    }
}
impl Deref for AsyncDevice {
    type Target = wgpu::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}
impl<T> AsRef<T> for AsyncDevice
where
    T: ?Sized,
    <AsyncDevice as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
