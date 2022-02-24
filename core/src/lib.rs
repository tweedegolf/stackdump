#![doc = include_str!("../README.md")]

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

#[cfg(any(feature = "std", doc))]
pub mod device_memory;
pub mod memory_region;
pub mod register_data;

pub use gimli;
