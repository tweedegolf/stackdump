#![no_main]
#![no_std]

use core::mem::MaybeUninit;
use cortex_m::peripheral::NVIC;
use embedded_hal::timer::CountDown;
use nrf52840_hal::pac::interrupt;
use rtt_target::{rprintln, rtt_init, UpChannel};
use stackdump_capture::core::memory_region::{ArrayMemoryRegion, SliceMemoryRegion};
use stackdump_capture::core::register_data::ArrayRegisterData;

#[link_section = ".uninit"]
static mut STACKDUMP: MaybeUninit<ArrayMemoryRegion<4096>> = MaybeUninit::uninit();
#[link_section = ".uninit"]
static mut CORE_REGISTERS: MaybeUninit<ArrayRegisterData<16, u32>> = MaybeUninit::uninit();
#[link_section = ".uninit"]
static mut FPU_REGISTERS: MaybeUninit<ArrayRegisterData<32, u32>> = MaybeUninit::uninit();

const MESSAGES: [&'static str; 4] = [
    "I love you",
    "I hate you",
    "I am indifferent to you",
    "I like you",
];

static mut DUMP_RTT_CHANNEL: Option<UpChannel> = None;

pub enum Speed {
    Full,
    Half,
    None,
}

pub enum TestMode {
    PrintSelective {
        with_address: bool,
        with_value: bool,
        num_val: u32,
    },
    PrintAll(u32),
}

#[repr(transparent)]
pub struct TransparentTest {
    value: u32,
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let _cp = cortex_m::Peripherals::take().unwrap();
    let dp = nrf52840_hal::pac::Peripherals::take().unwrap();

    let channels = rtt_init! {
        up: {
            0: {
                size: 1024
                mode: BlockIfFull
                name: "Terminal"
            }
            1: {
                size: 1024
                mode: BlockIfFull
                name: "Dump"
            }
        }
    };

    unsafe { DUMP_RTT_CHANNEL = Some(channels.up.1) }
    rtt_target::set_print_channel(channels.up.0);

    rprintln!("Generating interrupts");

    let mut rng = nrf52840_hal::Rng::new(dp.RNG);
    let random_index = rng.random_u32() % 4;
    let message = MESSAGES[random_index as usize];
    let increment = TransparentTest {
        value: random_index + 1,
    };
    rprintln!("increment: {:p} - {}", &increment.value, increment.value);

    let random_speed = match rng.random_u32() % 3 {
        0 => Speed::Full,
        1 => Speed::Half,
        2 => Speed::None,
        _ => unreachable!(),
    };

    let test_mode = match rng.random_u32() % 5 {
        0 => TestMode::PrintAll(rng.random_u32() % 2000 + 9000),
        1 => TestMode::PrintSelective {
            with_address: false,
            with_value: false,
            num_val: rng.random_u32() % 2000 + 9000,
        },
        2 => TestMode::PrintSelective {
            with_address: true,
            with_value: false,
            num_val: rng.random_u32() % 2000 + 9000,
        },
        3 => TestMode::PrintSelective {
            with_address: false,
            with_value: true,
            num_val: rng.random_u32() % 2000 + 9000,
        },
        4 => TestMode::PrintSelective {
            with_address: true,
            with_value: true,
            num_val: rng.random_u32() % 2000 + 9000,
        },
        _ => unreachable!(),
    };

    unsafe {
        NVIC::unmask(nrf52840_hal::pac::Interrupt::TIMER0);
    }

    let mut timer = nrf52840_hal::Timer::periodic(dp.TIMER0);
    timer.enable_interrupt();
    timer.start(200000u32);

    let res = do_loop(&increment, true, message, random_speed, test_mode);

    rprintln!("{}", res);

    loop {}
}

#[inline(never)]
fn do_loop(
    increment: &TransparentTest,
    double_trouble: bool,
    message: &str,
    speed: Speed,
    test_mode: TestMode,
) -> f64 {
    let mut num = 0;
    let mut nums = [0, 0, 0, 0];
    let mut fnum = 0.0;

    loop {
        if double_trouble {
            num += increment.value * 2;
        } else {
            num += increment.value;
        }
        nums[(num / increment.value) as usize % nums.len()] += increment.value;

        match speed {
            Speed::Full => fnum += 0.1,
            Speed::Half => fnum += 0.05,
            Speed::None => fnum += 0.0,
        }

        match test_mode {
            TestMode::PrintSelective {
                with_address: true,
                with_value: true,
                num_val,
            }
            | TestMode::PrintAll(num_val) => {
                if num % num_val == 0 || fnum > 100000000000000.0 {
                    rprintln!("num: {:p} - {}", &num, num);
                    rprintln!("nums: {:p} - {:?}", &nums, nums);
                    rprintln!("fnum: {:p} - {}", &fnum, fnum);
                    rprintln!("Message: {:p} {:p} - {}", &message, message, message);
                }
            }
            TestMode::PrintSelective {
                with_address: false,
                with_value: true,
                num_val,
            } => {
                if num % num_val == 0 || fnum > 100000000000000.0 {
                    rprintln!("num: {}", num);
                    rprintln!("nums: {:?}", nums);
                    rprintln!("fnum: {}", fnum);
                    rprintln!("Message: {}", message);
                }
            }
            TestMode::PrintSelective {
                with_address: true,
                with_value: false,
                num_val,
            } => {
                if num % num_val == 0 || fnum > 100000000000000.0 {
                    rprintln!("num: {:p}", &num);
                    rprintln!("nums: {:p}", &nums);
                    rprintln!("fnum: {:p}", &fnum);
                    rprintln!("Message: {:p} {:p}", &message, message);
                }
            }
            TestMode::PrintSelective {
                with_address: false,
                with_value: false,
                ..
            } => {}
        }

        if num > u32::MAX - increment.value {
            break fnum;
        }
    }
}

fn get_data_section_dump() -> SliceMemoryRegion<'static> {
    extern "C" {
        static mut __sdata: u32;
        static mut __edata: u32;
    }

    unsafe {
        let start = &__sdata as *const u32 as u32;
        let end = &__edata as *const u32 as u32;

        rprintln!("Data section: {:#10X}..{:#10X}", start, end);

        let mut section = SliceMemoryRegion::default();
        section.copy_from_memory(start as *const u8, (end - start) as usize);
        section
    }
}

fn get_bss_section_dump() -> SliceMemoryRegion<'static> {
    extern "C" {
        static __sbss: u32;
        static __ebss: u32;
    }

    unsafe {
        let start = &__sbss as *const u32 as u32;
        let end = &__ebss as *const u32 as u32;

        rprintln!("Bss section: {:#10X}..{:#10X}", start, end);

        let mut section = SliceMemoryRegion::default();
        section.copy_from_memory(start as *const u8, (end - start) as usize);
        section
    }
}

fn get_uninit_section_dump() -> SliceMemoryRegion<'static> {
    extern "C" {
        static mut __suninit: u32;
        static mut __euninit: u32;
    }

    unsafe {
        let start = &__suninit as *const u32 as u32;
        let end = &__euninit as *const u32 as u32;

        rprintln!("Uninit section: {:#10X}..{:#10X}", start, end);

        let mut section = SliceMemoryRegion::default();
        section.copy_from_memory(start as *const u8, (end - start) as usize);
        section
    }
}

#[interrupt]
fn TIMER0() {
    let timer = unsafe { &*nrf52840_hal::pac::TIMER0::ptr() };
    rprintln!("Timer interrupt!");
    // Stop the interrupt
    timer.events_compare[0].write(|w| w);

    unsafe {
        cortex_m::interrupt::free(|cs| {
            let stack = &mut *STACKDUMP.as_mut_ptr();
            let core_registers = &mut *CORE_REGISTERS.as_mut_ptr();
            let fpu_registers = &mut *FPU_REGISTERS.as_mut_ptr();
            stackdump_capture::cortex_m::capture(stack, core_registers, fpu_registers, cs);

            for byte in core_registers.bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in fpu_registers.bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in stack.bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in get_data_section_dump().bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in get_bss_section_dump().bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in get_uninit_section_dump().bytes() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
        });
    }

    panic!();
}

#[cortex_m_rt::exception]
unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("{:?}", frame);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rprintln!("{}", info);
    loop {
        cortex_m::asm::bkpt();
    }
}
