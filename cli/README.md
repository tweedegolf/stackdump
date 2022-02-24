# Stackdump-cli

Command line program for tracing dumps.

Also see the [main repo readme](../README.md).

This is made mostly for convenience and as an example for using the trace crate.
If you need to trace inside your own software, please use the library and not this CLI.

The program can be installed with:
```sh
cargo install stackdump-cli
```

Then you can use the help to see your options:

```sh
stackdump-cli --help
```

The cli only supports dumps in the format of the byte iterator.
You can have multiple memory regions and register datas in one file.

## Example

To trace a cortex-m dump, use the following command:
```sh
# Generic
stackdump-cli cortex-m <ELF_FILE> [DUMP_FILES..]

# Specific
stackdump-cli cortex-m .\examples\data\nrf52840 .\examples\data\nrf52840.dump
```