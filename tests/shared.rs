#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::single_component_path_imports)]
use test;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;

use std::sync::Arc;

use wgpu_async::{AsyncDevice, AsyncQueue};

fn block_on<F: std::future::Future<Output = ()> + 'static>(f: F) {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(f);
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(f);
}

async fn setup() -> (AsyncDevice, AsyncQueue) {
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
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                label: None,
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("missing device");
    // TODO: Look at swapping to `Rc` on web
    #[allow(clippy::arc_with_non_send_sync)]
    let (device, queue) = (Arc::new(device), Arc::new(queue));
    wgpu_async::wrap(Arc::clone(&device), Arc::clone(&queue))
}

#[self::test]
fn await_map_blocks_until_mapped() {
    block_on(async {
        let (device, _) = setup().await;

        let async_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 8192,
            usage: wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Map and unmap with async api
        async_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read)
            .await
            .unwrap();

        // Due to await, buffer can be read here
        let bytes = async_buffer.slice(..).get_mapped_range().to_vec();

        async_buffer.unmap();

        assert_eq!(bytes.len(), 8192);
    })
}

#[self::test]
fn await_map_write_copy_await_map_read() {
    block_on(async {
        let (device, queue) = setup().await;

        let async_buffer1 = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128 * 1024,
            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let async_buffer2 = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128 * 1024,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Write some data
        let data = (0..(128 * 1024)).map(|v| v as u8).collect::<Vec<_>>();
        async_buffer1
            .slice(..)
            .map_async(wgpu::MapMode::Write)
            .await
            .unwrap();
        async_buffer1
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(&data);
        async_buffer1.unmap();

        // Submit copy command
        let mut commands =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        commands.copy_buffer_to_buffer(&async_buffer1, 0, &async_buffer2, 0, 128 * 1024);
        queue.submit(vec![commands.finish()]).await;

        // Read copied data
        async_buffer2
            .slice(..)
            .map_async(wgpu::MapMode::Read)
            .await
            .unwrap();
        let read_data = async_buffer2.slice(..).get_mapped_range().to_vec();

        assert_eq!(read_data, data);
    });
}
