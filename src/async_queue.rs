use crate::async_device::{AsyncDevice, WgpuFuture};
use std::{ops::Deref, sync::Arc};
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

        self.device.with_debug_validation(
            move || {
                self.device.do_async(move |callback| {
                    queue_ref.submit(command_buffers);

                    queue_ref.on_submitted_work_done(|| callback(()));
                })
            },
            "command execution",
        )
    }

    pub fn device(&self) -> &AsyncDevice {
        &self.device
    }
}

// We're a smart pointer. Let everyone access our inner device.
impl Deref for AsyncQueue {
    type Target = wgpu::Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}
