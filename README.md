# Stackdump

A set of crates for capturing and tracing stack dumps.
See the docs of the respective operations.

| crate         | crates.io                                                                                                                     | docs                                                                                                           | Readme's                        |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| Core          | [![crates.io](https://img.shields.io/crates/v/stackdump-core.svg)](https://crates.io/crates/stackdump-core)                   | [![Documentation](https://docs.rs/stackdump-core/badge.svg)](https://docs.rs/stackdump-core)                   | [link](core/README.md)          |
| Capture       | [![crates.io](https://img.shields.io/crates/v/stackdump-capture.svg)](https://crates.io/crates/stackdump-capture)             | [![Documentation](https://docs.rs/stackdump-capture/badge.svg)](https://docs.rs/stackdump-capture)             | [link](capture/README.md)       |
| Capture-probe | [![crates.io](https://img.shields.io/crates/v/stackdump-capture-probe.svg)](https://crates.io/crates/stackdump-capture-probe) | [![Documentation](https://docs.rs/stackdump-capture-probe/badge.svg)](https://docs.rs/stackdump-capture-probe) | [link](capture-probe/README.md) |
| Trace         | [![crates.io](https://img.shields.io/crates/v/stackdump-trace.svg)](https://crates.io/crates/stackdump-trace)                 | [![Documentation](https://docs.rs/stackdump-trace/badge.svg)](https://docs.rs/stackdump-trace)                 | [link](trace/README.md)         |
| Cli           | [![crates.io](https://img.shields.io/crates/v/stackdump-cli.svg)](https://crates.io/crates/stackdump-cli)                     |                                                                                                                | [link](cli/README.md)           |

Currently only Cortex M is supported, but PR's are welcome!

There are likely many bugs in the tracing of variables. If you notice anything, please make a PR.
It would help if you can include the output of `readelf <your_elf_file> --debug-dump` in the issue (as a gist link).
For me to be fully able to reproduce the tracing, I will also need your elf file.

Both the debug dump and elf file can be sensitive for IP reasons. So if you can't include it in the issue, I can understand.

The output of the trace can look like this (with some spammy variables left out):

```text
0: stackdump_capture::cortex_m::capture_core_registers (InlineFunction)
  at C:\Repos\TG\stackdump\capture\src\cortex_m.rs:54:9

1: stackdump_capture::cortex_m::capture (InlineFunction)
  at C:\Repos\TG\stackdump\capture\src\cortex_m.rs:33:26

2: nrf52840::__cortex_m_rt_TIMER0::{{closure}} (InlineFunction)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:108:51

3: cortex_m::interrupt::free (Function)
  at C:\Users\diond\.cargo\registry\src\github.com-1ecc6299db9ec823\cortex-m-0.7.4\src\interrupt.rs:64:13
  variables:
    primask: Error(Optimized away (No location attribute)) (Primask) at C:\Users\diond\.cargo\registry\src\github.com-1ecc6299db9ec823\cortex-m-0.7.4\src\interrupt.rs:59

4: nrf52840::__cortex_m_rt_TIMER0 (InlineFunction)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:125:5

5: TIMER0 (Exception)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:98:1

6: <u64 as core::ops::bit::BitOrAssign>::bitor_assign (InlineFunction)
  at /rustc/532d3cda90b8a729cd982548649d32803d265052/library/core/src/ops/bit.rs:799:53

7: compiler_builtins::float::add::add (InlineFunction)
  at /cargo/registry/src/github.com-1ecc6299db9ec823/compiler_builtins-0.1.70/src/float/add.rs:177:5

8: compiler_builtins::float::add::__adddf3 (Function)
  at /cargo/registry/src/github.com-1ecc6299db9ec823/compiler_builtins-0.1.70/src/float/add.rs:201:9

9: nrf52840::do_loop (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:83:9
  variables:
    (parameter) increment: Error(Optimized away (No location attribute)) (&u32) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:71
    (parameter) double_trouble: true (bool) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:71
    (parameter) message: *0x0000C707:10 (= "I like you") (&str) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:71
    num: 310368 (u32) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:72
    nums: [77588, 0, 77592, 0] ([u32;4]) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:73
    fnum: Error(Location list not found for the current PC value (A variable lower on the stack may contain the value)) (f64) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:74

10: nrf52840::__cortex_m_rt_main (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs
  variables:
    channels: Error(Location list not found for the current PC value (A variable lower on the stack may contain the value)) (Channels) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:29
    random_index: Error(Location list not found for the current PC value (A variable lower on the stack may contain the value)) (u32) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:50
    message: *0x0000C707:10 (= "I like you") (&str) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:51
    increment: 4 (u32) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:52
    res: 4.24397352e-315 (f64) at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:63

11: main (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:24:1

12: RESET (Function)
```
