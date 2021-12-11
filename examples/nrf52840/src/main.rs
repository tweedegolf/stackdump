#![no_main]
#![no_std]
#![feature(asm)]

use core::mem::MaybeUninit;
use cortex_m::peripheral::NVIC;
use embedded_hal::timer::CountDown;
use nrf52840_hal::pac::interrupt;
use rtt_target::{rprintln, rtt_init_print};
use stackdump_capture::cortex_m::CortexMTarget;
use stackdump_capture::stackdump_core::Stackdump;

#[link_section = ".uninit"]
static mut STACKDUMP: MaybeUninit<Stackdump<CortexMTarget, 2048>> = MaybeUninit::uninit();

#[cortex_m_rt::entry]
fn main() -> ! {
    let _cp = cortex_m::Peripherals::take().unwrap();
    let dp = nrf52840_hal::pac::Peripherals::take().unwrap();

    rtt_init_print!(BlockIfFull);
    rprintln!("Generating interrupts");

    let mut rng = nrf52840_hal::Rng::new(dp.RNG);
    let increment = (rng.random_u32() % 4) + 1;
    rprintln!("increment: {:p} - {}", &increment, increment);

    unsafe {
        NVIC::unmask(nrf52840_hal::pac::Interrupt::TIMER0);
    }

    let mut timer = nrf52840_hal::Timer::periodic(dp.TIMER0);
    timer.enable_interrupt();
    timer.start(100000u32);

    let res = do_loop(&increment);

    rprintln!("{}", res);

    loop {

    }
}

#[inline(never)]
fn do_loop(increment: &u32) -> f64 {
    let mut num = 0;
    let mut nums = [0, 0, 0, 0];
    let mut fnum = 0.0;

    loop {
        num += increment;
        nums[(num / increment) as usize % nums.len()] += increment;
        fnum += 0.01;

        if num % 10000u32 == 0 {
            rprintln!("num: {:p} - {}", &num, num);
            rprintln!("nums: {:p} - {:?}", &nums, nums);
            rprintln!("fnum: {:p} - {}", &fnum, fnum);
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
        let dump = STACKDUMP.assume_init_mut();
        dump.capture();

        #[inline(never)]
        fn write_dump<const STACK_SIZE: usize>(dump: &mut Stackdump<CortexMTarget, STACK_SIZE>) {
            let mut buffer = [0; 80000];
            let size = serde_json_core::to_slice(&dump, &mut buffer).unwrap();
            rprintln!("\n{:X?}\n", dump.stack);
            rprintln!("{}", core::str::from_utf8(&buffer[..size]).unwrap());
        }

        write_dump(dump);
    }

    cortex_m::asm::bkpt();
}

#[cortex_m_rt::exception]
unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
    cortex_m::asm::bkpt();
    panic!("{:?}", frame);
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rprintln!("{}", info);
    loop {
        cortex_m::asm::bkpt();
    }
}
