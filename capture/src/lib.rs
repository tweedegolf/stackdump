#![no_std]
#![feature(asm)]

#[cfg(feature = "cortex-m")]
mod cortex_m;
#[cfg(feature = "cortex-m")]
pub use cortex_m::*;

pub use stackdump_core;
