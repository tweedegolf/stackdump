# Stackdump Trace

This crate implement stack tracing from the memory that was captured using the stackdump-capture crate.

Also see the [main repo readme](../README.md).

The aim is to extract as much information from the captured memory as possible.
As such, not only the stack frames are given, but also the variables that are present in each frame.
The value of these variables can only be read if their memory is captured.

The minimum this crate needs is a stack & registers capture.
But if any variable is found that points outside the stack, like a String, then you´ll need
to have captured the heap memory as well. Otherwise it will just show the pointer value.

Right now, only the cortex m target is supported.
A lot of the code could be refactored to work cross-platform.

If you want to add a target, then please discuss and create an issue or PR.

## Example

In this case we have a cortex m target with FPU.
A dump has been made with the two register captures first and then the stack capture.

```rust
let dump: Vec<u8> = // Get your dump from somewhere
let elf: Vec<u8> = // Read your elf file

let mut dump_iter = dump.iter().copied();

let mut device_memory = DeviceMemory::new();

device_memory.add_register_data(VecRegisterData::from_iter(&mut dump_iter));
device_memory.add_register_data(VecRegisterData::from_iter(&mut dump_iter));
device_memory.add_memory_region(VecMemoryRegion::from_iter(&mut dump_iter));

let frames = cortex_m::trace(device_memory, &elf).unwrap();
for (i, frame) in frames.iter().enumerate() {
    println!("{}: {}", i, frame);
}
```

## Reading live from the device

In principle, if you have a way of reading the memory of the device directly (e.g. via probe-rs),
then it is possible to create types that implement `RegisterData` and `MemoryRegion` so that you can
insert those into the `DeviceMemory` instance.
