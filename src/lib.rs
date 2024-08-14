#![deny(missing_docs)]
#![warn(clippy::cast_lossless)]
#![deny(clippy::cast_possible_truncation)]
#![deny(clippy::cast_possible_wrap)]
#![deny(clippy::cast_sign_loss)]
#![warn(clippy::cast_precision_loss)]
#![deny(clippy::as_underscore)]
#![deny(clippy::cast_ptr_alignment)]
#![warn(clippy::checked_conversions)]
#![warn(clippy::clone_on_ref_ptr)]
#![warn(clippy::cloned_instead_of_copied)]
#![warn(clippy::get_unwrap)]
#![deny(clippy::fallible_impl_from)]
#![warn(clippy::significant_drop_in_scrutinee)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::as_ptr_cast_mut)]
#![warn(clippy::case_sensitive_file_extension_comparisons)]
#![doc=include_str!( "../README.md")]

mod async_buffer;
mod async_device;
mod async_queue;
mod wgpu_future;

use std::sync::Arc;

pub use async_buffer::AsyncBuffer;
pub use async_buffer::AsyncBufferSlice;
pub use async_device::AsyncDevice;
pub use async_queue::AsyncQueue;
pub use wgpu_future::WgpuFuture;

/// Takes a regular `wgpu::Device` and `wgpu::Queue` and gives you the corresponding smart
/// pointers, [`AsyncDevice`] and [`AsyncQueue`].
///
/// # Usage
///
/// ```
/// # use std::sync::Arc;
/// # pollster::block_on(async {
/// let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
/// let adapter = instance
///     .request_adapter(&wgpu::RequestAdapterOptions {
///         power_preference: wgpu::PowerPreference::HighPerformance,
///         compatible_surface: None,
///         force_fallback_adapter: true,
///     })
///     .await
///     .expect("missing adapter");
/// let (device, queue) = adapter
///     .request_device(
///         &wgpu::DeviceDescriptor {
///             required_features: wgpu::Features::empty(),
///             required_limits: adapter.limits(),
///             label: None,
///         },
///         None,
///     )
///     .await
///     .expect("missing device");
///
/// let (device, queue) = (Arc::new(device), Arc::new(queue));
///
/// let (async_device, async_queue) = wgpu_async::wrap(
///     Arc::clone(&device),
///     Arc::clone(&queue)
/// );
///
/// // Then we can do some async-enabled things:
/// let async_buffer = async_device.create_buffer(&wgpu::BufferDescriptor {
///     label: None,
///     size: 8192,
///     usage: wgpu::BufferUsages::MAP_READ,
///     mapped_at_creation: false,
/// });
/// async_buffer.slice(..).map_async(wgpu::MapMode::Read).await; // New await functionality!
/// # })
/// ```
pub fn wrap(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> (AsyncDevice, AsyncQueue) {
    let device = AsyncDevice::new(device);
    let queue = AsyncQueue::new(device.clone(), queue);

    (device, queue)
}
