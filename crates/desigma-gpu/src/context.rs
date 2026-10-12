//! Headless device creation (tests, benchmarks, `--screenshot --gpu`). The
//! app itself reuses the device eframe already created.

use std::fmt;

#[derive(Debug)]
pub enum GpuError {
    /// No adapter could be created (typical on CI machines without a GPU).
    NoAdapter(String),
    Device(String),
    /// The device lacks something the renderer needs.
    Unsupported(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuError::NoAdapter(e) => write!(f, "no GPU adapter: {e}"),
            GpuError::Device(e) => write!(f, "GPU device error: {e}"),
            GpuError::Unsupported(e) => write!(f, "GPU feature unsupported: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// A device and queue created without a window.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl GpuContext {
    /// Requests the default adapter (hardware preferred). Software adapters are
    /// accepted: they are slow but exercise the same code.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
            .await
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor { label: Some("desigma-gpu"), required_limits: adapter.limits(), ..Default::default() })
            .await
            .map_err(|e| GpuError::Device(e.to_string()))?;
        Ok(Self { device, queue, adapter_info })
    }
}
