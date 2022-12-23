use crate::async_device::AsyncDevice;
use futures::{Future, FutureExt};
use std::ops::RangeBounds;
use wgpu::{Buffer, BufferAddress, BufferAsyncError, BufferSlice, MapMode};

#[derive(Debug)]
pub struct AsyncBuffer
where
    Self: Send,
{
    device: AsyncDevice,
    buffer: Buffer,
}

impl AsyncBuffer {
    pub fn new(device: AsyncDevice, buffer: Buffer) -> Self {
        Self { device, buffer }
    }

    /// Maps this buffer, in the same way a call to `buffer.slice(...).map_async(mode, ...)` would,
    /// except does so asynchronously, and returns a future that can be awaited to get the mapped slice.
    pub fn map_slice<'a, S: RangeBounds<BufferAddress>>(
        &'a self,
        bounds: S,
        mode: MapMode,
    ) -> impl Future<Output = Result<BufferSlice<'a>, BufferAsyncError>> + 'a {
        let slice = self.buffer.slice(bounds);

        self.device
            .do_async(|callback| slice.map_async(mode, callback))
            .map(move |v| v.map(|_| slice))
    }
}

impl AsRef<Buffer> for AsyncBuffer {
    fn as_ref(&self) -> &Buffer {
        &self.buffer
    }
}
