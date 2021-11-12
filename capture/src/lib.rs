#![no_std]
#![feature(asm)]

#[cfg(feature = "cortex-m")]
pub mod cortex_m;

pub use stackdump_core;
