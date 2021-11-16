#![no_main]
#![no_std]

use core::mem::MaybeUninit;
use cortex_m::peripheral::NVIC;
use embedded_hal::timer::CountDown;
use nrf52840_hal::pac::interrupt;
use rtt_target::{rprintln, rtt_init_print};
use stackdump_capture::cortex_m::CortexMTarget;
use stackdump_capture::stackdump_core::Stackdump;

#[link_section = ".uninit"]
static mut STACKDUMP: MaybeUninit<Stackdump<CortexMTarget, { 128 * 1024 }>> = MaybeUninit::uninit();

#[cortex_m_rt::entry]
fn main() -> ! {
    let _cp = cortex_m::Peripherals::take().unwrap();
    let dp = nrf52840_hal::pac::Peripherals::take().unwrap();

    rtt_init_print!(BlockIfFull);
    rprintln!("Generating interrupts");

    unsafe {
        NVIC::unmask(nrf52840_hal::pac::Interrupt::TIMER0);
    }

    let mut timer = nrf52840_hal::Timer::periodic(dp.TIMER0);
    timer.enable_interrupt();
    timer.start(100000u32);

    do_loop();
}

#[inline(never)]
fn do_loop() -> ! {
    let mut num = 0;

    loop {
        num += 1;

        if num % 10000u32 == 0 {
            rprintln!("{}", num);
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
        rprintln!("Dump: {:02X?}", dump);
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
