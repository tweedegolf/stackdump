# Changelog

## Unreleased

- Fix small crash when evaluating a variable location.
  - Now it will accurately report that the memory doesn't exist instead of returning an error.
- *Breaking*: Add range to memory region
- Fix issue where arrays and maybe some objects always looked at the first couple of data bits instead of the correct position

## 0.10.1 (24-07-25)

- Fix accidental use of std

## 0.10.0 (24-07-25)

- Updated a boatload of dependencies
- Started filtering many useless variables from the list of static variables
  - This means this isn't spammed full with defmt stuff anymore
- All subcrates now use the same version

## Cli 0.1.8, Capture 0.5.0, Core 0.4.0, Trace 0.4.0, Capture-probe 0.4.0 (21-08-23)

- Updated gimli to 0.28
- Updated Addr2line to 0.21
- Updated to probe-rs 0.20
- Fixed bug where the abstract origin didn't get handled
- Volatile and const types can now be traced (relevant for C)

## Cli 0.1.7 (19-05-23)

- Update to trace 0.3.0
- Move to env-logger for better logging

## Trace 0.3.0 (19-05-23)

- *Breaking*: TraceError is now non-exhaustive and has more variants
- Update to Addr2line 0.20.0
- Now works with more debug info situations. (I think a recent LLVM update might have changed some things where `DebugInfoRef`s are now generated instead of the `UnitRef`s before)

## Capture 0.4.0 (07-04-23)

- *Breaking*: Capturing no longer takes a critical section. I'm now convinced this is not necessary.

## Capture-probe 0.3.0 (07-04-23)

- *Breaking*: Update to probe-rs 0.18

## Cli 0.1.6 (07-04-23)

- Update to probe-rs 0.18
- Update to clap 4

## Core 0.3.0 (07-04-23)

- *Breaking*: Updated to gimli 0.27.2

## Trace 0.2.4 (07-04-23)

- Updated dependencies

## Core 0.2.2 (01-09-22)

- The length of the iterators for `MemoryRegion` and `RegisterData` always returned the original value. Now they return how many elements are left.

## Core 0.2.1 (29-08-22)

- The byte iterators for `MemoryRegion` and `RegisterData` now implement `ExactSizeIterator`.

## Capture-probe 0.2.0 (26-07-22)

- Updated to probe-rs 0.13

## Cli 0.1.5 (26-07-22)

- Updated to probe-rs 0.13

## Trace 0.2.3 (26-07-22)

- Implemented th RequiresMemory location step

## Trace 0.2.2 (12-07-22)

- Fixed an issue where objects were sometimes rendered with white text instead of the correct color

## Trace 0.2.1 (08-07-22)

- Fixed an issue where stack unwinding would think it reached the end too soon
- Added back in newlines for variable printouts
- Can now do the RequiresEntryValue step

## Capture 0.3.0 (28-06-22)
- *Breaking*: Updated to Core 0.2.0

## Cli 0.1.4 (28-06-22)

- Added colorized output, which can be specified with the `-t` option
- Added the ability to capture and trace from a running device using probe-rs

## Capture-probe 0.1.0 (28-06-22)

- Created an adaptor for letting a probe-rs core be used as MemoryRegion
- Added functions to capture the registers via probe-rs

## Trace 0.2.0 (28-06-22)

- *Breaking*: Big refactor to make the type decoding and value reading be structured instead of it all being strings
- *Breaking*: Tracing has been made crossplatform with an implementation for Cortex-M
- *Breaking*: Added new archetype: `typedef`
- Added reading capability for tagged unions (fancy Rust enums)
- Added color theme system for outputs
- Made it so that transparent types can be added to lessen the clutter in the trace
- Object member pointers (objects with the `DW_AT_containing_type` attribute) are now detected an not displayed by default. This hides all of the vtables.
- Subroutines now display a `_` instead of an `Unknown` error
- Strings longer than 64kb are no longer read to improve performance

## Core 0.2.0 (28-06-22)

- *Breaking*: Simplified the MemoryRegion trait and made it fallible
- *Breaking*: Simplified the RegisterData trait
- *Breaking*: DeviceMemory now takes a `'memory` lifetime so that not all data has to be owned
- *Breaking*: The FromIterator impl taking `&u8` has been removed leaving only one impl that takes `u8`. Just pass an iterator with `.copied()` to it.

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
