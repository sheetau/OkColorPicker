//! Test-only GPU readback for FP16 and unencoded 8-bit targets.
use zgui_render_wgpu::{Gpu, wgpu};
pub fn read_float(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<[f32; 4]> {
    let float = texture.format() == wgpu::TextureFormat::Rgba16Float;
    let stride = if float { 8 } else { 4 };
    let row = (texture.width() * stride).div_ceil(256) * 256;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    gpu.queue().submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    gpu.wait();
    receive.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let mut result = Vec::new();
    for y in 0..texture.height() {
        for x in 0..texture.width() {
            let start = (y * row + x * stride) as usize;
            result.push(std::array::from_fn(|c| {
                if float {
                    half::f16::from_le_bytes([mapped[start + c * 2], mapped[start + c * 2 + 1]])
                        .to_f32()
                } else {
                    let channel = if texture.format() == wgpu::TextureFormat::Bgra8Unorm {
                        [2, 1, 0, 3][c]
                    } else {
                        c
                    };
                    f32::from(mapped[start + channel]) / 255.0
                }
            }));
        }
    }
    drop(mapped);
    buffer.unmap();
    result
}
