use crate::{async_device::AsyncDevice, AsyncQueue, OutOfMemoryError};
use futures::{Future, FutureExt};
use std::ops::{Deref, DerefMut, RangeBounds};
use wgpu::{Buffer, BufferAddress, BufferAsyncError, BufferSlice, MapMode};

#[derive(Debug)]
pub struct AsyncBuffer
where
    Self: Send,
{
    pub(crate) label: Option<String>,
    pub(crate) device: AsyncDevice,
    pub(crate) buffer: Buffer,
}

impl AsyncBuffer {
    /// Maps this buffer, in the same way a call to `buffer.slice(...).map_async(mode, ...)` would,
    /// except does so asynchronously, and returns a future that can be awaited to get the mapped slice.
    pub fn map_slice<'a, S: RangeBounds<BufferAddress>>(
        &'a self,
        bounds: S,
        mode: MapMode,
    ) -> impl Future<Output = Result<BufferSlice<'a>, BufferAsyncError>> + Send + 'a {
        let slice = self.buffer.slice(bounds);

        self.device
            .do_async(|callback| slice.map_async(mode, callback))
            .map(move |v| v.map(|_| slice))
    }

    /// A descriptor that can be used to create a buffer exactly like this one
    pub fn descriptor(&self) -> wgpu::BufferDescriptor {
        wgpu::BufferDescriptor {
            label: self.label.as_ref().map(String::as_str),
            size: self.buffer.size(),
            usage: self.buffer.usage(),
            mapped_at_creation: false,
        }
    }

    /// Like the std function `Clone`, but async because we're chatting to the GPU
    pub async fn try_duplicate(&self, queue: &AsyncQueue) -> Result<Self, OutOfMemoryError> {
        let buffer = self.device.create_buffer(&self.descriptor()).await?;
        queue.copy_max(self, 0, &buffer, 0, None).await;
        Ok(buffer)
    }
}

// We're a smart pointer. Let everyone access our inner device.
impl Deref for AsyncBuffer {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for AsyncBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}
