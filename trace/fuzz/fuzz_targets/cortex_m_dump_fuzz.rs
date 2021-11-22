#![no_main]
use libfuzzer_sys::fuzz_target;
use stackdump_trace::stackdump_capture::cortex_m::CortexMTarget;
use stackdump_trace::stackdump_core::Stackdump;
use stackdump_trace::Trace;

const ELF: &[u8] = include_bytes!("../../../examples/data/nrf52840");
const STACK_SIZE: usize = 32768;

fuzz_target!(|stackdump: Stackdump<CortexMTarget, STACK_SIZE>| {
    let _ = stackdump.trace(ELF);
});
