pub mod async_buffer;
pub mod async_device;
pub mod async_queue;

pub fn wrap_wgpu(
    device: wgpu::Device,
    queue: wgpu::Queue,
) -> (async_device::AsyncDevice, async_queue::AsyncQueue) {
    let device = async_device::AsyncDevice::new(device);
    let queue = async_queue::AsyncQueue::new(device.clone(), queue);

    return (device, queue);
}
