#![cfg_attr(not(feature = "std"), no_std)]
#![feature(asm)]

#[cfg(feature = "cortex-m")]
pub mod cortex_m;

pub use stackdump_core;
