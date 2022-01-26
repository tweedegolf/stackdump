#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub mod device_memory;
pub mod memory_region;
pub mod register_data;
