//! # Stackdump Core
//!
//! This crate contains definitions for memory regions and register data.
//!
//! The [Capture](https://crates.io/crates/stackdump-capture) crate can capture the runtime data and registers into these types.
//! To get traces from the captured memory, use the [Trace](https://crates.io/crates/stackdump-trace) crate.
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

#[cfg(any(feature = "std", doc))]
pub mod device_memory;
pub mod memory_region;
pub mod register_data;

pub use gimli;
