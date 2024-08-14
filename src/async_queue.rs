use crate::{async_device::AsyncDevice, WgpuFuture};
use std::{ops::Deref, sync::Arc};
use wgpu::{CommandBuffer, Queue};

/// A wrapper around a [`wgpu::Queue`] which shadows some methods to allow for callback-and-poll
/// methods to be made async, such as [`AsyncQueue::submit`].
#[derive(Clone, Debug)]
pub struct AsyncQueue {
    device: AsyncDevice,
    queue: Arc<Queue>,
}

impl AsyncQueue {
    pub(crate) fn new(device: AsyncDevice, queue: Arc<Queue>) -> Self {
        Self { device, queue }
    }

    /// This is an `async` version of [`wgpu::Queue::submit`].
    ///
    /// Just like [`wgpu::Queue::submit`], a call to this method starts the given work immediately,
    /// however this method returns a future that can be awaited giving the completion of the submitted work.
    pub fn submit<I: IntoIterator<Item = CommandBuffer>>(
        &self,
        command_buffers: I,
    ) -> WgpuFuture<()> {
        let queue_ref = Arc::clone(&self.queue);

        queue_ref.submit(command_buffers);

        self.device.do_async(move |callback| {
            queue_ref.on_submitted_work_done(|| callback(()));
        })
    }

    /// Gets the device associated with this queue.
    pub fn device(&self) -> &AsyncDevice {
        &self.device
    }
}
impl Deref for AsyncQueue {
    type Target = wgpu::Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}
impl<T> AsRef<T> for AsyncQueue
where
    T: ?Sized,
    <AsyncQueue as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
