use crate::Arguments;
use probe_rs::{config::TargetSelector, DebugProbeSelector, Probe, Session, Architecture};
use stackdump_capture_probe::StackdumpCapturer;
use stackdump_trace::{
    platform::cortex_m::CortexMPlatform, stackdump_core::device_memory::DeviceMemory,
};
use std::{error::Error, path::Path, time::Duration};

pub(crate) fn trace_probe(
    elf_file: &Path,
    probe_selector: Option<DebugProbeSelector>,
    target_selector: TargetSelector,
    core: Option<usize>,
    args: &Arguments,
) -> Result<(), Box<dyn Error>> {
    let elf_data = std::fs::read(elf_file)?;

    let mut session = match probe_selector {
        Some(selector) => Probe::open(selector)?.attach(target_selector)?,
        None => Session::auto_attach(target_selector)?,
    };
    let mut core = session.core(core.unwrap_or(0))?;

    // TODO on master: Change this to use the core_type
    let core_type = core.architecture();
    // TODO on master
    // let fpu_supported = core.fpu_support()?;
    core.halt(Duration::from_secs(2))?;

    let mut stackcapturer = StackdumpCapturer::new(&mut core);

    let mut device_memory = DeviceMemory::new();
    device_memory.add_register_data(stackcapturer.capture_core_registers()?);

    // TODO on master
    // if fpu_supported { 
    //     if let Some(fpu_registers) = stackcapturer.capture_fpu_registers()? {
    //         device_memory.add_register_data(fpu_registers);
    //     }
    // }

    device_memory.add_memory_region(stackcapturer);

    if matches!(core_type, Architecture::Arm) {
        let frames = stackdump_trace::platform::trace::<CortexMPlatform>(device_memory, &elf_data)?;
        crate::print_frames(frames, args);
    } else {
        unimplemented!("Other tracing than on cortex-m is not yet implemented");
    }

    core.run()?;

    Ok(())
}
