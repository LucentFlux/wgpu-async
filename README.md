# WGPU-Async

[![crates.io](https://img.shields.io/crates/v/wgpu-async.svg)](https://crates.io/crates/wgpu-async)
[![docs.rs](https://img.shields.io/docsrs/wgpu-async)](https://docs.rs/wgpu-async/latest/wgpu_async/)
[![crates.io](https://img.shields.io/crates/l/wgpu-async.svg)](https://github.com/LucentFlux/wgpu-async/blob/main/LICENSE)

This crate adds a global poll loop thread on non-WASM platforms that can be used to create a `WgpuFuture` holding the completion of a task. The poll loop is conservative, parking itself when no futures are waiting on it, meaning that this crate adds as little overhead as possible in changing paradigms.

Note that this crate does not aim to improve the performance of anything, and fast applications should reduce CPU-GPU communication and synchronisation as much as possible, irrespective of the paradigm used and platform targetted. In fact, when used incorrectly, this crate can drastically decrease performance, as shown under the [Common Pitfall](#Common-Pitfall) section. This crate is intended for use when prototyping or testing, when parity between native and web targets is more important than speed.

## Motivation

[WGPU](https://github.com/gfx-rs/wgpu) offers some `async` methods when initialising adapters and devices, but during program execution much of the timing between the CPU and GPU is managed through callbacks and polling. A common pattern is to do something like the following:

```rust ignore
wgpu.do_something();
wgpu.on_something_done(|result| { /* Handle results */ });
wgpu.poll();
```

This is a very JavaScript-esque pattern, while in Rust we might expect to write code that looks more like:

```rust ignore
let result = wgpu.do_something().await;
```

Or, if we still wanted a callback:

```rust ignore
wgpu.do_something().then(|result| { /* Handle results */ }).await;
```

Also, on Web targets we find that the call to `poll` is entirely unnecessary, increasing conceptual complexity in programs which target both Native and Web. This crate unifies the two under a common `async/await` API.

## Common Pitfall

Due to the polling thread running both intermittently and globaly, independently from other parts of your code, it is possible that using this library may mask errors when performing operations that must be awaited. For example, the following code *should* deadlock:

```rust compile_fail
// BAD CODE - DON'T DO THIS
let (sender, receiver) = flume::bounded(1);
let mapping = wgpu::Buffer::slice(buffer, ..).map_async(.., |_| sender.send(()));
// ERROR: Poll not called, so buffer will never map, so recv will never complete.
receiver.recv().unwrap();
```

However with this library, the call to `map_async` might eventually go through (if you are using futures elsewhere), but in an unknown amount of time, causing possibly huge performance losses. It is assumed that those performance losses will be noticable enough that the bug will be found, but you should be aware that use of this crate has the potential to hide deadlocks behind performance hits.

## Usage

To do things in an `async` way, your `wgpu::Device` and `wgpu::Queue` need to be wrapped in async smart-pointer versions. These implement `Deref<Device>` and `Deref<Queue>`, so can be used as a slot-in replacement for existing wgpu code. 

```rust ignore
// Create a device and queue like normal
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

// Make them async
let (device, queue) = wgpu_async::wrap(Arc::new(device), Arc::new(queue));
```

Then you can use shadowed `wgpu` methods with the exact same signatures, but with extra `async`-ness:

```rust ignore
queue.submit(&[/* commands */]).await; // An awaitable `Queue::submit`!
```

Just like their base `wgpu` counterparts, these methods begin their work on the GPU immediately. However the device won't begin to be polled until the future is awaited.

You can also convert any non-shadowed callback-and-poll method to an async one using `AsyncDevice::do_async`:

```rust ignore
wgpu.do_something();
let future = device.do_async(move |callback| {
    wgpu.on_something_done(|result| callback(result));
});

let result = future.await;
```
