#![doc = include_str!("../README.md")]

use clap::{Parser, Subcommand};
use colored::Colorize;
use probe::trace_probe;
use probe_rs::DebugProbeSelector;
use stackdump_trace::{
    platform::cortex_m::CortexMPlatform,
    stackdump_core::{
        device_memory::DeviceMemory,
        memory_region::{VecMemoryRegion, MEMORY_REGION_IDENTIFIER},
        register_data::{VecRegisterData, REGISTER_DATA_IDENTIFIER},
    },
};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

mod probe;

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
    #[clap(about = "Trace from files using Cortex-M as the target")]
    CortexM {
        #[clap(help = "Path to the elf file with debug info")]
        elf_file: PathBuf,
        #[clap(
            min_values = 1,
            help = "The memory dumps. Must be in the format of the byte iterator in the core crate. Multiple dumps can be put into the file."
        )]
        dumps: Vec<PathBuf>,
    },
    #[clap(about = "Trace by capturing the data from the probe")]
    Probe {
        #[clap(help = "Path to the elf file with debug info")]
        elf_file: PathBuf,
        #[clap(short = 'c', long = "chip", help = "The target chip specifier")]
        chip: String,
        #[clap(
            short = 'p',
            long = "probe",
            help = "The probe to use (default is the first found probe)"
        )]
        probe: Option<DebugProbeSelector>,
        #[clap(long = "core", help = "The core to trace (default is core 0)")]
        core: Option<usize>,
    },
}

fn main() {
    let start = std::time::Instant::now();

    match result_main() {
        Ok(_) => {}
        Err(e) => {
            println!("Error: {e}");
        }
    }

    println!("\nDone in {:.03} seconds", start.elapsed().as_secs_f32());
}

fn result_main() -> Result<(), Box<dyn Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Off)
        .env()
        .with_colors(true)
        .init()
        .unwrap();

    let args = Arguments::parse();

    match &args.platform {
        Platform::CortexM { elf_file, dumps } => {
            let (elf_data, device_memory) = read_files_into_device_memory(elf_file, dumps)?;
            let frames =
                stackdump_trace::platform::trace::<CortexMPlatform>(device_memory, &elf_data)?;
            print_frames(frames, &args);
        }
        Platform::Probe {
            elf_file,
            probe,
            chip,
            core,
        } => {
            trace_probe(&elf_file, probe.clone(), chip.into(), *core, &args)?;
        }
    }

    Ok(())
}

pub(crate) fn print_frames(frames: Vec<stackdump_trace::Frame<u32>>, args: &Arguments) {
    let terminal_size = termsize::get();
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
                    && line.chars().count() > terminal_size.cols as usize * args.max_wrapping_lines
                {
                    println!(
                        "{}{}",
                        truncate(line, terminal_size.cols as usize * args.max_wrapping_lines),
                        format!(
                            "... ({} more)",
                            div_ceil(line.chars().count(), terminal_size.cols as usize)
                                - args.max_wrapping_lines
                        )
                        .dimmed()
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

fn read_files_into_device_memory(
    elf_file: &Path,
    dumps: &[PathBuf],
) -> Result<(Vec<u8>, DeviceMemory<'static, u32>), Box<dyn Error>> {
    let elf_data = std::fs::read(elf_file)?;
    let mut device_memory = DeviceMemory::new();
    for dump_path in dumps {
        let dump_data = std::fs::read(dump_path)?;

        let mut dump_iter = dump_data.into_iter().peekable();

        while let Some(id) = dump_iter.peek().cloned() {
            match id {
                MEMORY_REGION_IDENTIFIER => {
                    device_memory.add_memory_region(VecMemoryRegion::from_iter(&mut dump_iter))
                }
                REGISTER_DATA_IDENTIFIER => {
                    device_memory.add_register_data(VecRegisterData::from_iter(&mut dump_iter))
                }
                _ => return Err("Dump data error. Got to an unexpected identifier".into()),
            }
        }
    }
    Ok((elf_data, device_memory))
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
