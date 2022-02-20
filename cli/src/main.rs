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
    #[clap(subcommand)]
    platform: Platform,
}

#[derive(Subcommand, Debug)]
enum Platform {
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
        .env()
        .with_colors(true)
        .init()
        .unwrap();

    let args = Arguments::parse();

    match args.platform {
        Platform::CortexM { elf_file, dumps } => {
            let elf_data = std::fs::read(elf_file)?;

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
                        _ => Err(format!("Dump data error. Got to an unexpected identifier"))?,
                    }
                }
            }

            let frames = stackdump_trace::cortex_m::trace(device_memory, &elf_data)?;

            for (i, frame) in frames.iter().enumerate() {
                println!("{}: {}", i, frame);
            }
        }
    }

    Ok(())
}
