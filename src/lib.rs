pub mod async_buffer;
pub mod async_device;
pub mod async_queue;

pub use async_buffer::AsyncBuffer;
pub use async_device::AllocationScope;
pub use async_device::AsyncDevice;
pub use async_device::OutOfMemoryError;
pub use async_device::WgpuFuture;
pub use async_queue::AsyncQueue;

/// Takes a regular `wgpu::Device` and `wgpu::Queue` and gives you the corresponding smart
/// pointers, [`AsyncDevice`] and [`AsyncQueue`].
///
/// # Usage
///
/// ```
/// # pollster::block_on(async {
/// let instance = wgpu::Instance::new(wgpu::Backends::all());
/// let adapter = instance
///     .request_adapter(&wgpu::RequestAdapterOptions {
///         power_preference: wgpu::PowerPreference::HighPerformance,
///         compatible_surface: None,
/// #       force_fallback_adapter: true,
///     })
///     .await
///     .expect("missing adapter");
/// let (device, queue) = adapter
///     .request_device(
///         &wgpu::DeviceDescriptor {
///             features: wgpu::Features::empty(),
///             limits: adapter.limits(),
///             label: None,
///         },
///         None,
///     )
///     .await
///     .expect("missing device");
///
/// let (device, queue) = wgpu_async::wrap_wgpu(device, queue);
/// # })
/// ```
pub fn wrap_to_async(device: wgpu::Device, queue: wgpu::Queue) -> (AsyncDevice, AsyncQueue) {
    let device = AsyncDevice::new(device);
    let queue = AsyncQueue::new(device.clone(), queue);

    return (device, queue);
}
