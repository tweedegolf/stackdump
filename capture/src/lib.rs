#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![cfg_attr(avr, feature(asm_experimental_arch))]
#[cfg(any(cortex_m, doc, test))]
pub mod cortex_m;
#[cfg(any(avr, doc, test))]
pub mod avr;

pub use stackdump_core as core;
