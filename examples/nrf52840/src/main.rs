#![no_main]
#![no_std]

use cortex_m::peripheral::NVIC;
use rtt_target::{rprintln, rtt_init_print};
use embedded_hal::timer::CountDown;
use nrf52840_hal::pac::interrupt;
use stackdump_capture::stackdump_core::Registers;

#[cortex_m_rt::entry]
fn main() -> ! {
    let _cp = cortex_m::Peripherals::take().unwrap();
    let dp = nrf52840_hal::pac::Peripherals::take().unwrap();

    rtt_init_print!();
    rprintln!("Generating interrupt");

    unsafe { NVIC::unmask(nrf52840_hal::pac::Interrupt::TIMER0); }

    let mut timer = nrf52840_hal::Timer::periodic(dp.TIMER0);
    timer.enable_interrupt();
    timer.start(1000000u32);

    do_loop();
}

#[inline(never)]
fn do_loop() -> ! {
    let mut num = 0;

    loop {
        num += 1;

        if num % 100000u32 == 0 {
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

    let mut regs = stackdump_capture::CortexMRegisters::default();
    regs.capture();
    rprintln!("Registers: {:02X?}", regs);

    cortex_m::asm::bkpt();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    rprintln!("{}", info);
    loop {
        cortex_m::asm::bkpt();
    }
}
