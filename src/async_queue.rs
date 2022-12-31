use crate::async_device::{AsyncDevice, WgpuFuture};
use std::sync::Arc;
use wgpu::{CommandBuffer, Queue};

#[derive(Clone, Debug)]
pub struct AsyncQueue {
    device: AsyncDevice,
    queue: Arc<Queue>,
}

impl AsyncQueue {
    pub fn new(device: AsyncDevice, queue: Queue) -> Self {
        Self {
            device,
            queue: Arc::new(queue),
        }
    }

    /// Starts work immediately on call, but work is only finished when returned future is awaited
    pub fn submit<I: IntoIterator<Item = CommandBuffer> + Send>(
        &self,
        command_buffers: I,
    ) -> WgpuFuture<()> {
        let queue_ref = self.queue.clone();
        let device_ref = self.device.clone();
        self.device.do_async(move |callback| {
            // We add validation on debug builds
            #[cfg(debug_assertions)]
            device_ref
                .as_ref()
                .push_error_scope(wgpu::ErrorFilter::Validation);

            queue_ref.submit(command_buffers);

            // Just fail fast if the debug validation scope was invalidated
            #[cfg(debug_assertions)]
            pollster::block_on(device_ref.as_ref().pop_error_scope()).expect("validation error");

            queue_ref.on_submitted_work_done(|| callback(()));
        })
    }
}

impl AsRef<Queue> for AsyncQueue {
    fn as_ref(&self) -> &Queue {
        &self.queue
    }
}
