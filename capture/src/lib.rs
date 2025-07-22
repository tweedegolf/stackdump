#![doc = include_str!("../README.md")]
#![no_std]
#![warn(missing_docs)]

#[cfg(any(cortex_m, doc, test))]
pub mod cortex_m;
pub use stackdump_core as core;
