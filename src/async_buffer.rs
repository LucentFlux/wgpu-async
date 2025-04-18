use crate::{async_device::AsyncDevice, WgpuFuture};
use std::ops::{Deref, DerefMut, RangeBounds};
use wgpu::{BufferAddress, BufferAsyncError};

/// A wrapper around a [`wgpu::Buffer`] which shadows some methods to allow for async
/// mapping using Rust's `async` API.
#[derive(Debug)]
pub struct AsyncBuffer
where
    Self: wgpu::WasmNotSend,
{
    pub(crate) device: AsyncDevice,
    pub(crate) buffer: wgpu::Buffer,
}

impl AsyncBuffer {
    /// Takes a slice of this buffer, in the same way a call to [`wgpu::Buffer::slice`] would,
    /// except wraps the result in an [`AsyncBufferSlice`] so that the `map_async` method can be
    /// awaited.
    pub fn slice<S: RangeBounds<BufferAddress>>(&self, bounds: S) -> AsyncBufferSlice<'_> {
        let buffer_slice = self.buffer.slice(bounds);
        AsyncBufferSlice {
            device: self.device.clone(),
            buffer_slice,
        }
    }
}
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
impl<T> AsRef<T> for AsyncBuffer
where
    T: ?Sized,
    <AsyncBuffer as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
impl<T> AsMut<T> for AsyncBuffer
where
    <AsyncBuffer as Deref>::Target: AsMut<T>,
{
    fn as_mut(&mut self) -> &mut T {
        self.deref_mut().as_mut()
    }
}

/// A smart-pointer wrapper around a [`wgpu::BufferSlice`], offering a `map_async` method than can be `await`ed.
#[derive(Debug)]
pub struct AsyncBufferSlice<'a>
where
    Self: wgpu::WasmNotSend,
{
    device: AsyncDevice,
    buffer_slice: wgpu::BufferSlice<'a>,
}
impl AsyncBufferSlice<'_> {
    /// An awaitable version of [`wgpu::Buffer::map_async`].
    pub fn map_async(&self, mode: wgpu::MapMode) -> WgpuFuture<Result<(), BufferAsyncError>> {
        self.device
            .do_async(|callback| self.buffer_slice.map_async(mode, callback))
    }
}
impl<'a> Deref for AsyncBufferSlice<'a> {
    type Target = wgpu::BufferSlice<'a>;

    fn deref(&self) -> &Self::Target {
        &self.buffer_slice
    }
}
impl DerefMut for AsyncBufferSlice<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer_slice
    }
}
impl<T> AsRef<T> for AsyncBufferSlice<'_>
where
    T: ?Sized,
    <Self as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}
impl<T> AsMut<T> for AsyncBufferSlice<'_>
where
    <Self as Deref>::Target: AsMut<T>,
{
    fn as_mut(&mut self) -> &mut T {
        self.deref_mut().as_mut()
    }
}
