use crate::async_device::{AsyncDevice, WgpuFuture};
use std::{ops::Deref, sync::Arc};
use wgpu::{BufferAddress, CommandBuffer, Queue, COPY_BUFFER_ALIGNMENT};

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
                queue_ref.submit(command_buffers);

                self.device.do_async(move |callback| {
                    queue_ref.on_submitted_work_done(|| callback(()));
                })
            },
            "command execution",
        )
    }

    pub fn device(&self) -> &AsyncDevice {
        &self.device
    }

    /// Copies as much of one buffer into another buffer as is possible, up to an optional `max_length` parameter.
    ///
    /// # Panics
    ///
    /// Panics if the length to copy is not a multiple of [`wgpu::COPY_BUFFER_ALIGNMENT`]
    pub fn copy_max(
        &self,
        source: &wgpu::Buffer,
        source_offset: BufferAddress,
        destination: &wgpu::Buffer,
        destination_offset: BufferAddress,
        max_length: Option<BufferAddress>,
    ) -> WgpuFuture<()> {
        let mut copy_command_encoder = self.device.create_command_encoder(&Default::default());

        let mut length = u64::min(
            source.size() - source_offset,
            destination.size() - destination_offset,
        );
        if let Some(max_length) = max_length {
            length = u64::max(length, max_length);
        }

        if length == 0 {
            // Future that awaits nothing
            let dud_future = self.device.do_async(|callback| callback(()));
            return dud_future;
        }

        debug_assert_eq!(
            length % COPY_BUFFER_ALIGNMENT,
            0,
            "smaller buffer size must be a multiple of COPY_BUFFER_ALIGNMENT"
        );

        copy_command_encoder.copy_buffer_to_buffer(
            source,
            source_offset,
            destination,
            destination_offset,
            length,
        );
        self.submit(vec![copy_command_encoder.finish()])
    }
}

// We're a smart pointer. Let everyone access our inner device.
impl Deref for AsyncQueue {
    type Target = wgpu::Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}
