#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "cortex-m")]
pub mod cortex_m;

pub use stackdump_core;
