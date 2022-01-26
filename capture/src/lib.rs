#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(cortex_m)]
pub mod cortex_m;
pub use stackdump_core;
