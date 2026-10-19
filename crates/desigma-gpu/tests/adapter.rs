use desigma_gpu::{GpuContext, Renderer};

#[test]
fn probe_adapter() {
    match GpuContext::new() {
        Ok(c) => {
            eprintln!("adapter: {:?}", c.adapter_info);
            // Once a device exists, pipeline validation must pass. In particular,
            // uniform binding sizes must match the shaders rather than silently
            // turning a broken renderer into a skipped fidelity check.
            Renderer::new(&c.device, &c.queue).expect("renderer pipelines must validate");
        }
        Err(e) => eprintln!("SKIP: {e}"),
    }
}
