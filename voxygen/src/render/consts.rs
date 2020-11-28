use super::buffer::DynamicBuffer;
use bytemuck::Pod;

/// A handle to a series of constants sitting on the GPU. This is used to hold
/// information used in the rendering process that does not change throughout a
/// single render pass.
pub struct Consts<T: Copy + Pod> {
    buf: DynamicBuffer<T>,
}

impl<T: Copy + Pod> Consts<T> {
    /// Create a new `Const<T>`.
    pub fn new(device: &wgpu::Device, len: usize) -> Self {
        Self {
            // TODO: examine if all our consts need to be updateable
            buf: DynamicBuffer::new(device, len, wgpu::BufferUsage::UNIFORM),
        }
    }

    /// Update the GPU-side value represented by this constant handle.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vals: &[T],
        offset: usize,
    ) {
        self.buf.update(device, queue, vals, offset)
    }

    pub fn buf(&self) -> &wgpu::Buffer { &self.buf.buf }
}
