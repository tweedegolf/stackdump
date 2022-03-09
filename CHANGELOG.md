# Changelog

## Unreleased

- Static variables are now also traced
- Extra CLI option `-l` for capping how many times lines can wrap. This is useful for when tracing contains e.g. a long array type
- Added `SliceMemoryRegion` that can act as a memory region, but always borrows all its data. This is useful for when the region can't be owned, but is not referenced anywhere else

## 0.1.1 (24-02-22)

- Improved the docs a tiny bit. This release is mainly done because docs.rs failed to build the crates due to an outage.

## 0.1.0 (24-02-22)

- Initial release
