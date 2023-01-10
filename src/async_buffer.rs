use crate::async_device::AsyncDevice;
use futures::{Future, FutureExt};
use std::ops::{Deref, DerefMut, RangeBounds};
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
