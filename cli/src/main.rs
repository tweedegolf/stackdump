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
    }, render_colors::Theme,
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
    #[clap(
        short = 't',
        long,
        help = "The color theme of the outputted text",
        default_value_t = Theme::Dark,
    )]
    theme: Theme,
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
    for (i, frame) in frames.iter().enumerate() {
        print!("{}: ", i);

        let frame_text = frame.display(
            true,
            args.show_inlined_variables,
            args.show_zero_sized_variables,
            args.theme,
        );

        let line_wrapping_options = textwrap::Options::with_termwidth()
            .wrap_algorithm(textwrap::WrapAlgorithm::new_optimal_fit())
            .subsequent_indent("      ")
            .break_words(false)
            .word_separator(textwrap::WordSeparator::AsciiSpace);

        let max_lines = if args.max_wrapping_lines == 0 {
            usize::MAX
        } else {
            args.max_wrapping_lines
        };

        for frame_line in frame_text.lines() {
            let wrapping_lines = textwrap::wrap(frame_line, line_wrapping_options.clone());
            let wrapping_lines_count = wrapping_lines.len();

            for (wrapping_line_index, wrapping_line) in wrapping_lines.iter().enumerate() {
                println!("{wrapping_line}");

                if wrapping_line_index == max_lines - 1 {
                    println!(
                        "      {}",
                        format!("... ({} more)", wrapping_lines_count - max_lines).dimmed()
                    );
                    break;
                }
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
