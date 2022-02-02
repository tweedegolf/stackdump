# Stackdump

A set of crates for capturing and tracing stack dumps.
See the docs of the respective operations.

| crate   | crates.io                                                                                                         | docs                                                                                               |
| ------- | ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Core    | [![crates.io](https://img.shields.io/crates/v/stackdump-core.svg)](https://crates.io/crates/stackdump-core)       | [![Documentation](https://docs.rs/stackdump-core/badge.svg)](https://docs.rs/stackdump-core)       |
| Capture | [![crates.io](https://img.shields.io/crates/v/stackdump-capture.svg)](https://crates.io/crates/stackdump-capture) | [![Documentation](https://docs.rs/stackdump-capture/badge.svg)](https://docs.rs/stackdump-capture) |
| Trace   | [![crates.io](https://img.shields.io/crates/v/stackdump-trace.svg)](https://crates.io/crates/stackdump-trace)     | [![Documentation](https://docs.rs/stackdump-trace/badge.svg)](https://docs.rs/stackdump-trace)     |

Currently only Cortex M is supported, but PR's are welcome!

There are likely many bugs in the tracing of variables. If you notice anything, please make a PR.
It would help if you can include the output of `readelf <your_elf_file> --debug-dump` in the issue (as a gist link).
For me to be fully reproduce the tracing I will also need your elf file.

Both the debug dump and elf file can be sensitive for IP reasons. So if you can't include it in the issue, I can understand.

The output of the trace can look like this (with some spammy variables left out):

```text
0: stackdump_capture::cortex_m::capture_core_registers (Function)
  at C:\Repos\TG\stackdump\capture\src\cortex_m.rs:29:9

1: stackdump_capture::cortex_m::capture (Function)
  at C:\Repos\TG\stackdump\capture\src\cortex_m.rs:8:5

2: stackdump_capture::cortex_m::capture_with_fpu (Function)
  at C:\Repos\TG\stackdump\capture\src\cortex_m.rs:18:6

3: nrf52840::__cortex_m_rt_TIMER0::{{closure}} (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:107:51
  variables:
    param: { (ZST) } ({closure#1})
    cs: *0x2003FBEC (= CriticalSection { _0: () } (CriticalSection)) (&CriticalSection)

4: cortex_m::interrupt::free (Function)
  at C:\Users\diond\.cargo\registry\src\github.com-1ecc6299db9ec823\cortex-m-0.7.4\src\interrupt.rs:64:13
  variables:
    f: { (ZST) } ({closure#1})
    primask: Primask::Active (Primask)
    r: { (ZST) } (())

5: nrf52840::__cortex_m_rt_TIMER0 (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:105:9
  variables:
    timer: *0x40008000 (= Error(Not within available memory) (RegisterBlock)) (&RegisterBlock)

6: TIMER0 (Exception)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:97:1

7: compiler_builtins::float::add::__adddf3 (Function)
  at /cargo/registry/src/github.com-1ecc6299db9ec823/compiler_builtins-0.1.66/src/macros.rs:228:10

8: nrf52840::do_loop (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:82:9
  variables:
    increment: *0x2003FF2C (= 2 (u32)) (&u32)
    double_trouble: true (bool)
    message: *0x0000EA9A:10 (= "I hate you") (&str)
    num: 79684 (u32)
    nums: [19920, 0, 19922, 0] ([u32;4])
    fnum: 199.1999999999638 (f64)
    _args: (&&u32, &u32) { __0: *0x2003FD8C (= *0x2003FD48 (= 79684 (u32)) (&u32)), __1: *0x2003FD48 (= 79684 (u32)) } ((&&u32, &u32))
    channels: Channels { up: (rtt_target::UpChannel, rtt_target::UpChannel) { __0: UpChannel { __0: *0x20000020 (= Error(Not within available memory) (RttChannel)) }, __1: UpChannel { __0: *0x20000038 (= Error(Not within available memory) (RttChannel)) } } } (Channels)
    rng: { (ZST) } (Rng)
    random_index: 1 (u32)
    message: *0x0000EA9A:10 (= "I hate you") (&str)
    increment: 2 (u32)
    _args: (&&u32, &u32) { __0: *0x2003FF60 (= *0x2003FF2C (= 2 (u32)) (&u32)), __1: *0x2003FF2C (= 2 (u32)) } ((&&u32, &u32))
    timer: { (ZST) } (Timer<nrf52840_pac::TIMER0, nrf_hal_common::timer::Periodic>)
    res: 1.4661482823948837e-84 (f64)
    _args: (&f64) { __0: *0xF36598AF (= Error(Not within available memory) (f64)) } ((&f64))

9: nrf52840::__cortex_m_rt_main (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:62:15
  variables:
    _cp: { (ZST) } (Peripherals)
    dp: { (ZST) } (Peripherals)
    cb: *0x20000008 (= Error(Not within available memory) (RttControlBlock)) (&mut RttControlBlock)
    name: *0x0000EA6C (= 84 (u8)) (*const u8)
    mode: ChannelMode::BlockIfFull (ChannelMode)
    name: *0x0000EA75 (= 68 (u8)) (*const u8)
    mode: ChannelMode::BlockIfFull (ChannelMode)
    channels: Channels { up: (rtt_target::UpChannel, rtt_target::UpChannel) { __0: UpChannel { __0: *0x20000020 (= Error(Not within available memory) (RttChannel)) }, __1: UpChannel { __0: *0x20000038 (= Error(Not within available memory) (RttChannel)) } } } (Channels)
    rng: { (ZST) } (Rng)
    random_index: 1 (u32)
    message: *0x0000EA9A:10 (= "I hate you") (&str)
    increment: 2 (u32)
    _args: (&&u32, &u32) { __0: *0x2003FF60 (= *0x2003FF2C (= 2 (u32)) (&u32)), __1: *0x2003FF2C (= 2 (u32)) } ((&&u32, &u32))
    timer: { (ZST) } (Timer<nrf52840_pac::TIMER0, nrf_hal_common::timer::Periodic>)
    res: 1.4661482823948837e-84 (f64)
    _args: (&f64) { __0: *0xF36598AF (= Error(Not within available memory) (f64)) } ((&f64))

10: main (Function)
  at C:\Repos\TG\stackdump\examples\nrf52840\src\main.rs:23:1

11: RESET (Function)
```
