use std::{
    ops::Deref,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use wgpu_async::{AsyncDevice, AsyncQueue};

fn setup() -> (AsyncDevice, AsyncQueue) {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: true,
            })
            .await
            .expect("missing adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: adapter.limits(),
                    label: None,
                },
                None,
            )
            .await
            .expect("missing device");
        let (device, queue) = (Arc::new(device), Arc::new(queue));
        wgpu_async::wrap(Arc::clone(&device), Arc::clone(&queue))
    })
}

#[test]
fn after_map_buffer_loop_stops() {
    let (device, _) = setup();

    let async_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 8192,
        usage: wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Map and unmap with async api, which polls on extra thread
    pollster::block_on(async {
        async_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read)
            .await
            .unwrap();
    });
    async_buffer.unmap();

    // Stop using the async api
    let buffer: &wgpu::Buffer = async_buffer.deref();
    let is_mapped = Arc::new(AtomicBool::from(false));

    // Call unmap, but don't poll
    let local_is_mapped = Arc::clone(&is_mapped);
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |_| {
        local_is_mapped.store(true, std::sync::atomic::Ordering::Release)
    });

    // Wait - buffer shouldn't map
    std::thread::sleep(Duration::from_secs(10));
    assert!(!is_mapped.load(std::sync::atomic::Ordering::Acquire));

    // Poll - buffer should map
    device.poll(wgpu::Maintain::Wait);
    assert!(is_mapped.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn after_submit_empty_commands_and_map_buffer_twice_loop_stops() {
    let (device, queue) = setup();

    let async_buffer1 = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 128 * 1024,
        usage: wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let async_buffer2 = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 128 * 1024,
        usage: wgpu::BufferUsages::MAP_WRITE,
        mapped_at_creation: false,
    });

    let commands1 = device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
        .finish();
    let commands2 = device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
        .finish();

    // Submit empty commands
    let q1 = queue.submit(vec![commands1]);
    let q2 = queue.submit(vec![commands2]);

    // Map
    let f1 = async_buffer1.slice(..).map_async(wgpu::MapMode::Read);
    let f2 = async_buffer2.slice(..).map_async(wgpu::MapMode::Write);

    pollster::block_on(async {
        q1.await;
        q2.await;
        f1.await.unwrap();
        f2.await.unwrap();
    });

    async_buffer1.unmap();
    async_buffer2.unmap();

    // Stop using the async api
    let buffer: &wgpu::Buffer = async_buffer1.deref();
    let is_mapped = Arc::new(AtomicBool::from(false));

    // Call unmap, but don't poll
    let local_is_mapped = Arc::clone(&is_mapped);
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |_| {
        local_is_mapped.store(true, std::sync::atomic::Ordering::Release)
    });

    // Wait - buffer shouldn't map
    std::thread::sleep(Duration::from_secs(10));
    assert!(!is_mapped.load(std::sync::atomic::Ordering::Acquire));

    // Poll - buffer should map
    device.poll(wgpu::Maintain::Wait);
    assert!(is_mapped.load(std::sync::atomic::Ordering::Acquire));
}
