# Changelog

## Unreleased

### Cli

- Added colorized output
- Added the ability to capture and trace from a running device using probe-rs

### Capture-probe

- Created an adaptor for letting a probe-rs core be used as MemoryRegion
- Added functions to capture the registers via probe-rs

### Trace

- *Breaking*: Big refactor to make the type decoding and value reading be structured instead of it all being strings
- Added reading capability for tagged unions (fancy Rust enums)
- Added colorized output
- Made it so that transparent types can be added to lessen the clutter in the trace
- Object member pointers (objects with the `DW_AT_containing_type` attribute) are now detected an not displayed by default. This hides all of the vtables.
- Subroutines now display a `_` instead of an `Unknown` error

### Core

- *Breaking*: Simplified the MemoryRegion trait and made it fallible
- *Breaking*: Simplified the RegisterData trait
- *Breaking*: DeviceMemory now takes a `'memory` lifetime so that not all data has to be owned

## Capture 0.2.0 (03-05-22)

- Changed the function signature of the capture function. It now takes references to existing register data collections instead of returning new ones to improve ergonomics.

## 0.1.3 (31-03-22)

- Fixed the CLI where it used `show_inlined_variables` instead of `show_zero_sized_variables`
- Added a couple more tags to ignore when searching for static variables
## 0.1.2 (09-03-22)

- Static variables are now also traced
- Extra CLI option `-l` for capping how many times lines can wrap. This is useful for when tracing contains e.g. a long array type
- Added `SliceMemoryRegion` that can act as a memory region, but always borrows all its data. This is useful for when the region can't be owned, but is not referenced anywhere else

## 0.1.1 (24-02-22)

- Improved the docs a tiny bit. This release is mainly done because docs.rs failed to build the crates due to an outage.

## 0.1.0 (24-02-22)

- Initial release
