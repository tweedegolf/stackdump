#![doc = include_str!("../README.md")]

use clap::{Parser, Subcommand};
use stackdump_trace::stackdump_core::{
    device_memory::DeviceMemory,
    memory_region::{VecMemoryRegion, MEMORY_REGION_IDENTIFIER},
    register_data::{VecRegisterData, REGISTER_DATA_IDENTIFIER},
};
use std::{error::Error, path::PathBuf};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Arguments {
    #[clap(
        subcommand,
        help = "The platform from which the stackdump was captured"
    )]
    platform: Platform,
    #[clap(short = 'i', long, help = "Print all traced inlined variables")]
    show_inlined_variables: bool,
    #[clap(short = 'z', long, help = "Print all traced zero-sized variables")]
    show_zero_sized_variables: bool,
    #[clap(
        short = 'l',
        long,
        help = "Cap the line length so it doesn't wrap more than the given amount of time. Use 0 for uncapped.",
        default_value_t = 5
    )]
    max_wrapping_lines: usize,
}

#[derive(Subcommand, Debug)]
enum Platform {
    #[clap(about = "Trace using Cortex-M as the target")]
    CortexM {
        #[clap(help = "Path to the elf file with debug info")]
        elf_file: PathBuf,
        #[clap(
            min_values = 1,
            help = "The memory dumps. Must be in the format of the byte iterator in the core crate. Multiple dumps can be put into the file."
        )]
        dumps: Vec<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Off)
        .env()
        .with_colors(true)
        .init()
        .unwrap();

    let args = Arguments::parse();

    match args.platform {
        Platform::CortexM { elf_file, dumps } => {
            // Read the elf file
            let elf_data = std::fs::read(elf_file)?;

            // Read the saved device memory
            let mut device_memory = DeviceMemory::new();
            for dump_path in dumps {
                let dump_data = std::fs::read(dump_path)?;

                let mut dump_iter = dump_data.into_iter().peekable();

                while let Some(id) = dump_iter.peek().cloned() {
                    match id {
                        MEMORY_REGION_IDENTIFIER => device_memory
                            .add_memory_region(VecMemoryRegion::from_iter(&mut dump_iter)),
                        REGISTER_DATA_IDENTIFIER => device_memory
                            .add_register_data(VecRegisterData::from_iter(&mut dump_iter)),
                        _ => return Err("Dump data error. Got to an unexpected identifier".into()),
                    }
                }
            }

            // Trace the stackdump
            let frames = stackdump_trace::cortex_m::trace(device_memory, &elf_data)?;

            let terminal_size = termsize::get();

            // Display the frames
            for (i, frame) in frames.iter().enumerate() {
                print!("{}: ", i);

                let frame_text = frame.display(
                    true,
                    args.show_inlined_variables,
                    args.show_zero_sized_variables,
                );

                for line in frame_text.lines() {
                    if let Some(terminal_size) = terminal_size.as_ref() {
                        if args.max_wrapping_lines != 0
                            && line.chars().count()
                                > terminal_size.cols as usize * args.max_wrapping_lines
                        {
                            println!(
                                "{}... ({} more)",
                                truncate(
                                    line,
                                    terminal_size.cols as usize * args.max_wrapping_lines
                                ),
                                div_ceil(line.chars().count(), terminal_size.cols as usize)
                                    - args.max_wrapping_lines
                            );
                        } else {
                            println!("{}", line);
                        }
                    } else {
                        println!("{}", line);
                    }
                }
            }
        }
    }

    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

fn div_ceil(lhs: usize, rhs: usize) -> usize {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}
