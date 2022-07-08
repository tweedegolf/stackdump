use crate::{error::TraceError, type_value_tree::TypeValueTree, Frame, FrameType, Location};
use addr2line::object::{Object, ObjectSection, SectionKind};
use funty::Fundamental;
use gimli::{DebugInfoOffset, EndianRcSlice, RunTimeEndian};
use stackdump_core::{device_memory::DeviceMemory, memory_region::VecMemoryRegion};
use std::collections::HashMap;

pub mod cortex_m;

/// The result of an unwinding procedure
pub enum UnwindResult<ADDR: funty::Integral> {
    /// The unwinding is done up to the start of the program
    Finished,
    /// The unwinding can't continue because the stack is corrupted
    Corrupted {
        /// An optional frame that explains the corruption
        error_frame: Option<Frame<ADDR>>,
    },
    /// The unwinding took another step and is not yet finished
    Proceeded,
}

pub trait Platform<'data> {
    type Word: funty::Integral;

    fn create_context(
        elf: &addr2line::object::File<'data, &'data [u8]>,
    ) -> Result<Self, TraceError>
    where
        Self: Sized;

    /// Unwind the stack of the platform to the previous exception if possible
    ///
    /// The device memory is mutated so that it is brought back to the state it was before the previous exception.
    ///
    /// Based on the unwinding, new information about the previous frame can be discovered.
    /// In that case, that frame can be updated with that info.
    fn unwind(
        &mut self,
        device_memory: &mut DeviceMemory<Self::Word>,
        previous_frame: Option<&mut Frame<Self::Word>>,
    ) -> Result<UnwindResult<Self::Word>, TraceError>;
}

/// Create the stacktrace for the given platform.
///
/// - device_memory: All the captured memory of the device.
/// It is not necessary to include any data that is present in the elf file because that will automatically be added.
/// It is required to have a decent chunk of the stack present. If not all of the stack is present,
/// then eventually the tracing procedure will find a corrupt frame.
/// The standard set of registers is also required to be present.
/// - elf_data: The raw bytes of the elf file.
/// This must be the exact same elf file as the one the device was running. Even a recompilation of the exact same code can change the debug info.
pub fn trace<'data, P: Platform<'data>>(
    mut device_memory: DeviceMemory<P::Word>,
    elf_data: &'data [u8],
) -> Result<Vec<Frame<P::Word>>, TraceError>
where
    <P::Word as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    // Parse the elf data
    let elf = addr2line::object::File::parse(elf_data)?;

    // Add all relevant memory sections present in the elf file to the device memory
    for section in elf.sections().filter(|section| {
        matches!(
            section.kind(),
            SectionKind::Text | SectionKind::ReadOnlyData | SectionKind::ReadOnlyString
        )
    }) {
        device_memory.add_memory_region(VecMemoryRegion::new(
            section.address(),
            section.uncompressed_data()?.to_vec(),
        ));
    }

    // Create the vector we'll be adding our found frames to
    let mut frames = Vec::new();

    // To find the frames, we need the addr2line context which does a lot of the work for us
    let addr2line_context = addr2line::Context::new(&elf)?;

    // To unwind, we need the platform context
    let mut platform_context = P::create_context(&elf)?;

    let mut type_cache = Default::default();

    // Now we need to keep looping until we unwound to the start of the program
    loop {
        // Get the frames of the current state
        match add_current_frames::<P>(
            &device_memory,
            &addr2line_context,
            &mut frames,
            &mut type_cache,
        ) {
            Ok(_) => {}
            Err(e @ TraceError::DwarfUnitNotFound { pc: _ }) => {
                frames.push(Frame {
                    function: "Unknown".into(),
                    location: Location::default(),
                    frame_type: FrameType::Corrupted(e.to_string()),
                    variables: Vec::default(),
                });
                break;
            }
            Err(e) => return Err(e),
        }

        // Try to unwind
        match platform_context.unwind(&mut device_memory, frames.last_mut())? {
            UnwindResult::Finished => {
                frames.push(Frame {
                    function: "RESET".into(),
                    location: crate::Location {
                        file: None,
                        line: None,
                        column: None,
                    },
                    frame_type: FrameType::Function,
                    variables: Vec::new(),
                });
                break;
            }
            UnwindResult::Corrupted {
                error_frame: Some(error_frame),
            } => {
                frames.push(error_frame);
                break;
            }
            UnwindResult::Corrupted { error_frame: None } => {
                break;
            }
            UnwindResult::Proceeded => {
                continue;
            }
        }
    }

    // We're done with the stack data, but we can also decode the static variables and make a frame out of that
    let static_variables = crate::variables::find_static_variables(
        addr2line_context.dwarf(),
        &device_memory,
        &mut type_cache,
    )?;
    let static_frame = Frame {
        function: "Static".into(),
        location: Location {
            file: None,
            line: None,
            column: None,
        },
        frame_type: FrameType::Static,
        variables: static_variables,
    };
    frames.push(static_frame);

    // We're done
    Ok(frames)
}

fn add_current_frames<'a, P: Platform<'a>>(
    device_memory: &DeviceMemory<P::Word>,
    addr2line_context: &addr2line::Context<EndianRcSlice<RunTimeEndian>>,
    frames: &mut Vec<Frame<P::Word>>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<P::Word>, TraceError>>,
) -> Result<(), TraceError>
where
    <P::Word as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    // Find the frames of the current register context
    let mut context_frames =
        addr2line_context.find_frames(device_memory.register(gimli::Arm::PC)?.as_u64())?;

    // Get the debug compilation unit of the current register context
    let unit = addr2line_context
        .find_dwarf_unit(device_memory.register(gimli::Arm::PC)?.as_u64())
        .ok_or(TraceError::DwarfUnitNotFound {
            pc: device_memory.register(gimli::Arm::PC)?.as_u64(),
        })?;

    // Get the abbreviations of the unit
    let abbreviations = addr2line_context.dwarf().abbreviations(&unit.header)?;

    // Loop through the found frames and add them
    let mut added_frames = 0;
    while let Some(context_frame) = context_frames.next()? {
        let (file, line, column) = context_frame
            .location
            .map(|l| {
                (
                    l.file.map(|f| f.to_string()),
                    l.line.map(|line| line as _),
                    l.column.map(|column| column as _),
                )
            })
            .unwrap_or_default();

        let mut variables = Vec::new();

        if let Some(die_offset) = context_frame.dw_die_offset {
            let mut entries = match unit.header.entries_tree(&abbreviations, Some(die_offset)) {
                Ok(entries) => entries,
                Err(_) => {
                    continue;
                }
            };

            if let Ok(entry_root) = entries.root() {
                variables = crate::variables::find_variables_in_function(
                    addr2line_context.dwarf(),
                    unit,
                    &abbreviations,
                    device_memory,
                    entry_root,
                    type_cache,
                )?;
            }
        }

        frames.push(Frame {
            function: context_frame
                .function
                .and_then(|f| f.demangle().ok().map(|f| f.into_owned()))
                .unwrap_or_else(|| "UNKNOWN".into()),
            location: crate::Location { file, line, column },
            frame_type: FrameType::InlineFunction,
            variables,
        });

        added_frames += 1;
    }

    if added_frames > 0 {
        // The last frame of `find_frames` is always a real function. All frames before are inline functions.
        frames.last_mut().unwrap().frame_type = FrameType::Function;
    }

    Ok(())
}
