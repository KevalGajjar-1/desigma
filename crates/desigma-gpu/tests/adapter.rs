use desigma_gpu::GpuContext;

#[test]
fn probe_adapter() {
    match GpuContext::new() {
        Ok(c) => eprintln!("adapter: {:?}", c.adapter_info),
        Err(e) => eprintln!("SKIP: {e}"),
    }
}
