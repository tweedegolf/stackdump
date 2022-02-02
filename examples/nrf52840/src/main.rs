#![no_main]
#![no_std]

use core::mem::MaybeUninit;
use cortex_m::peripheral::NVIC;
use embedded_hal::timer::CountDown;
use nrf52840_hal::pac::interrupt;
use rtt_target::{rprintln, rtt_init, UpChannel};
use stackdump_capture::stackdump_core::memory_region::ArrayMemoryRegion;

#[link_section = ".uninit"]
static mut STACKDUMP: MaybeUninit<ArrayMemoryRegion<4096>> = MaybeUninit::uninit();

const MESSAGES: [&'static str; 4] = [
    "I love you",
    "I hate you",
    "I am indifferent to you",
    "I like you",
];

static mut DUMP_RTT_CHANNEL: Option<UpChannel> = None;

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
    let increment = random_index + 1;
    rprintln!("increment: {:p} - {}", &increment, increment);

    unsafe {
        NVIC::unmask(nrf52840_hal::pac::Interrupt::TIMER0);
    }

    let mut timer = nrf52840_hal::Timer::periodic(dp.TIMER0);
    timer.enable_interrupt();
    timer.start(200000u32);

    let res = do_loop(&increment, true, message);

    rprintln!("{}", res);

    loop {}
}

#[inline(never)]
fn do_loop(increment: &u32, double_trouble: bool, message: &str) -> f64 {
    let mut num = 0;
    let mut nums = [0, 0, 0, 0];
    let mut fnum = 0.0;

    loop {
        if double_trouble {
            num += increment * 2;
        } else {
            num += increment;
        }
        nums[(num / increment) as usize % nums.len()] += increment;
        fnum += 0.01;

        if num % 10000u32 == 0 {
            rprintln!("num: {:p} - {}", &num, num);
            rprintln!("nums: {:p} - {:?}", &nums, nums);
            rprintln!("fnum: {:p} - {}", &fnum, fnum);
            rprintln!("Message: {:p} {:p} - {}", &message, message, message);
        }

        if num > u32::MAX - increment {
            break fnum;
        }
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
            let stack = STACKDUMP.write(ArrayMemoryRegion::default());
            let (core_registers, fpu_registers) = stackdump_capture::cortex_m::capture_with_fpu(stack, cs);
            rprintln!("{:2X?}", core_registers);
            rprintln!("{:2X?}", fpu_registers);
            rprintln!("Start of stack: {:#010X}", stack.start_address);

            for byte in core_registers.iter() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in fpu_registers.iter() {
                DUMP_RTT_CHANNEL.as_mut().unwrap().write(&[byte]);
            }
            for byte in stack.iter() {
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
