//! Trace implementation for the cortex m target

use crate::error::TraceError;
use crate::{Frame, FrameType, Location};
use addr2line::{
    object::{File, Object, ObjectSection, ObjectSymbol, SectionKind},
    Context,
};
use core::ops::Range;
use gimli::{
    BaseAddresses, CfaRule, DebugFrame, EndianRcSlice, EndianSlice, LittleEndian, RegisterRule,
    RunTimeEndian, UnwindContext, UnwindSection, UnwindTableRow,
};
use stackdump_core::{device_memory::DeviceMemory, memory_region::VecMemoryRegion};

mod variables;

pub(self) const THUMB_BIT: u32 = 1;

struct UnwindingContext<'data> {
    debug_frame: DebugFrame<EndianSlice<'data, LittleEndian>>,
    reset_vector_address_range: Range<u32>,
    text_address_range: Range<u32>,
    addr2line_context: Context<EndianRcSlice<RunTimeEndian>>,
    device_memory: DeviceMemory<u32>,
    bases: BaseAddresses,
    unwind_context: UnwindContext<EndianSlice<'data, LittleEndian>>,
}

impl<'data> UnwindingContext<'data> {
    pub fn create(
        elf: File<'data>,
        mut device_memory: DeviceMemory<u32>,
    ) -> Result<Self, TraceError> {
        let addr2line_context = addr2line::Context::new(&elf)?;

        let debug_info_sector_data = elf
            .section_by_name(".debug_frame")
            .ok_or_else(|| TraceError::MissingElfSection(".debug_frame".into()))?
            .data()?;
        let mut debug_frame =
            addr2line::gimli::DebugFrame::new(debug_info_sector_data, LittleEndian);
        debug_frame.set_address_size(std::mem::size_of::<u32>() as u8);

        let vector_table_section = elf
            .section_by_name(".vector_table")
            .ok_or_else(|| TraceError::MissingElfSection(".vector_table".into()))?;
        let vector_table = vector_table_section
            .data()?
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        let reset_vector_address = vector_table[1];
        let reset_vector_address_range = elf
            .symbols()
            .find(|sym| sym.address() as u32 == reset_vector_address)
            .map(|reset_vector_symbol| {
                reset_vector_symbol.address() as u32
                    ..reset_vector_symbol.address() as u32 + reset_vector_symbol.size() as u32
            })
            .unwrap_or(reset_vector_address..reset_vector_address);
        let text_section = elf
            .section_by_name(".text")
            .ok_or_else(|| TraceError::MissingElfSection(".text".into()))?;
        let text_address_range = (text_section.address() as u32)
            ..(text_section.address() as u32 + text_section.size() as u32);

        let bases = BaseAddresses::default();
        let unwind_context = UnwindContext::new();

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

        Ok(Self {
            debug_frame,
            reset_vector_address_range,
            text_address_range,
            addr2line_context,
            device_memory,
            bases,
            unwind_context,
        })
    }

    pub fn find_current_frames(&mut self, frames: &mut Vec<Frame<u32>>) -> Result<(), TraceError> {
        // Find the frames of the current register context
        let mut context_frames = self
            .addr2line_context
            .find_frames(self.device_memory.register(gimli::Arm::PC)? as u64)?;

        // Get the debug compilation unit of the current register context
        let unit = self
            .addr2line_context
            .find_dwarf_unit(self.device_memory.register(gimli::Arm::PC)? as u64)
            .ok_or(TraceError::DwarfUnitNotFound {
                pc: self.device_memory.register(gimli::Arm::PC)? as u64,
            })?;

        // Get the abbreviations of the unit
        let abbreviations = self.addr2line_context.dwarf().abbreviations(&unit.header)?;

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
                    variables = variables::find_variables_in_function(
                        self.addr2line_context.dwarf(),
                        unit,
                        &abbreviations,
                        &self.device_memory,
                        entry_root,
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

    /// Tries to unwind the stack.
    ///
    /// Returns the next frame and true if there are more frames or false if there are no more frames left
    pub fn try_unwind(
        &mut self,
        last_frame: Option<&mut Frame<u32>>,
    ) -> Result<(Option<Frame<u32>>, bool), TraceError> {
        let unwind_info = self.debug_frame.unwind_info_for_address(
            &self.bases,
            &mut self.unwind_context,
            self.device_memory.register(gimli::Arm::PC)? as u64,
            DebugFrame::cie_from_offset,
        );

        let unwind_info = match unwind_info {
            Ok(unwind_info) => unwind_info.clone(),
            Err(_e) => {
                return Ok((Some(Frame { function: "Unknown".into(), location: crate::Location { file: None, line: None, column: None }, frame_type: FrameType::Corrupted(format!("debug information for address {:#x} is missing. Likely fixes:
                1. compile the Rust code with `debug = 1` or higher. This is configured in the `profile.{{release,bench}}` sections of Cargo.toml (`profile.{{dev,test}}` default to `debug = 2`)
                2. use a recent version of the `cortex-m` crates (e.g. cortex-m 0.6.3 or newer). Check versions in Cargo.lock
                3. if linking to C code, compile the C code with the `-g` flag", self.device_memory.register(gimli::Arm::PC)?)),
                    variables: Vec::new(), }), false));
            }
        };

        // We can update the stackpointer and other registers to the previous frame by applying the unwind info
        let stack_pointer_changed = match self.apply_unwind_info(unwind_info) {
            Ok(stack_pointer_changed) => stack_pointer_changed,
            Err(e) => {
                return Ok((
                    Some(Frame {
                        function: "Unknown".into(),
                        location: crate::Location {
                            file: None,
                            line: None,
                            column: None,
                        },
                        frame_type: FrameType::Corrupted(e.to_string()),
                        variables: Vec::new(),
                    }),
                    false,
                ));
            }
        };

        // We're not at the last frame. What's the reason?

        // Do we have a corrupted stack?
        if !stack_pointer_changed
            && self.device_memory.register(gimli::Arm::LR)? & !THUMB_BIT
                == self.device_memory.register(gimli::Arm::PC)? & !THUMB_BIT
        {
            // The stack pointer didn't change and our LR points to our current PC
            // If we unwound further we'd get the same frame again so we better stop

            return Ok((
                Some(Frame {
                    function: "Unknown".into(),
                    location: crate::Location {
                        file: None,
                        line: None,
                        column: None,
                    },
                    frame_type: FrameType::Corrupted(
                        "CFA did not change and LR and PC are equal".into(),
                    ),
                    variables: Vec::new(),
                }),
                false,
            ));
        }

        // Stack is not corrupted, but unwinding is not done
        // Are we returning from an exception? (EXC_RETURN)
        if self.device_memory.register(gimli::Arm::LR)? > 0xffff_ffe0 {
            // Yes, so the registers were pushed to the stack and we need to get them back

            // Check the value to know if there are fpu registers to read
            let fpu = match self.device_memory.register(gimli::Arm::LR)? {
                0xFFFFFFF1 | 0xFFFFFFF9 | 0xFFFFFFFD => false,
                0xFFFFFFE1 | 0xFFFFFFE9 | 0xFFFFFFED => true,
                _ => {
                    return Ok((
                        Some(Frame {
                            function: "Unknown".into(),
                            location: crate::Location {
                                file: None,
                                line: None,
                                column: None,
                            },
                            frame_type: FrameType::Corrupted(format!(
                                "LR contains invalid EXC_RETURN value {:#10X}",
                                self.device_memory.register(gimli::Arm::LR)?
                            )),
                            variables: Vec::new(),
                        }),
                        false,
                    ));
                }
            };

            if let Some(last_frame) = last_frame {
                last_frame.frame_type = FrameType::Exception;
            }

            match self.update_registers_with_exception_stack(fpu) {
                Ok(()) => {}
                Err(TraceError::MissingMemory(address)) => {
                    return Ok((
                        Some(Frame {
                            function: "Unknown".into(),
                            location: crate::Location {
                                file: None,
                                line: None,
                                column: None,
                            },
                            frame_type: FrameType::Corrupted(format!(
                                "Could not read address {:#10X} from the stack",
                                address
                            )),
                            variables: Vec::new(),
                        }),
                        false,
                    ));
                }
                Err(e) => return Err(e),
            }
        } else {
            // No exception, so follow the LR back
            *self.device_memory.register_mut(gimli::Arm::PC)? =
                self.device_memory.register(gimli::Arm::LR)?
        }

        // Have we reached the reset vector?
        if self
            .reset_vector_address_range
            .contains(self.device_memory.register_ref(gimli::Arm::PC)?)
        {
            // Yes, let's make that a frame as well
            // We'll also make an assumption that there's no frames before reset
            return Ok((
                Some(Frame {
                    function: "RESET".into(),
                    location: crate::Location {
                        file: None,
                        line: None,
                        column: None,
                    },
                    frame_type: FrameType::Function,
                    variables: Vec::new(),
                }),
                false,
            ));
        }

        if self.is_last_frame()? {
            Ok((None, false))
        } else {
            // Is our stack pointer in a weird place?
            if self
                .device_memory
                .read_u32(
                    self.device_memory.register(gimli::Arm::SP)? as u64,
                    RunTimeEndian::Little,
                )
                .is_none()
            {
                Ok((Some(Frame {
                    function: "Unknown".into(),
                    location: crate::Location { file: None, line: None, column: None },
                    frame_type: FrameType::Corrupted(
                        format!("The stack pointer ({:#08X}) is corrupted or the dump does not contain the full stack", self.device_memory
                        .register(gimli::Arm::SP)?),
                    ),
                    variables: Vec::new(),
                }), false))
            } else {
                Ok((None, true))
            }
        }
    }

    fn apply_unwind_info(
        &mut self,
        unwind_info: UnwindTableRow<EndianSlice<LittleEndian>>,
    ) -> Result<bool, TraceError> {
        let updated = match unwind_info.cfa() {
            CfaRule::RegisterAndOffset { register, offset } => {
                let new_cfa = (self.device_memory.register(*register)? as i64 + *offset) as u32;
                let old_cfa = self.device_memory.register(gimli::Arm::SP)?;
                let changed = new_cfa != old_cfa;
                *self.device_memory.register_mut(gimli::Arm::SP)? = new_cfa;
                changed
            }
            CfaRule::Expression(_) => todo!("CfaRule::Expression"),
        };

        for (reg, rule) in unwind_info.registers() {
            match rule {
                RegisterRule::Undefined => unreachable!(),
                RegisterRule::Offset(offset) => {
                    let cfa = self.device_memory.register(gimli::Arm::SP)?;
                    let addr = (i64::from(cfa) + offset) as u64;
                    let new_value = self
                        .device_memory
                        .read_u32(addr, RunTimeEndian::Little)
                        .ok_or(TraceError::MissingMemory(addr))?;
                    *self.device_memory.register_mut(*reg)? = new_value;
                }
                _ => unimplemented!(),
            }
        }

        Ok(updated)
    }

    fn is_last_frame(&self) -> Result<bool, TraceError> {
        Ok(self.device_memory.register(gimli::Arm::LR)? == 0
            || (!self
                .text_address_range
                .contains(self.device_memory.register_ref(gimli::Arm::PC)?)
                && self.device_memory.register(gimli::Arm::LR)? <= 0xFFFF_FFE0))
    }

    /// Assumes we are at an exception point in the stack unwinding.
    /// Reads the registers that were stored on the stack and updates our current register representation with it.
    ///
    /// Returns Ok if everything went fine or an error with an address if the stack could not be read
    fn update_registers_with_exception_stack(&mut self, fpu: bool) -> Result<(), TraceError> {
        let current_sp = self.device_memory.register(gimli::Arm::SP)?;

        fn read_stack_var(
            device_memory: &DeviceMemory<u32>,
            starting_sp: u32,
            index: usize,
        ) -> Result<u32, TraceError> {
            device_memory
                .read_u32(starting_sp as u64 + index as u64 * 4, RunTimeEndian::Little)
                .ok_or(TraceError::MissingMemory(
                    starting_sp as u64 + index as u64 * 4,
                ))
        }

        *self.device_memory.register_mut(gimli::Arm::R0)? =
            read_stack_var(&self.device_memory, current_sp, 0)?;
        *self.device_memory.register_mut(gimli::Arm::R1)? =
            read_stack_var(&self.device_memory, current_sp, 1)?;
        *self.device_memory.register_mut(gimli::Arm::R2)? =
            read_stack_var(&self.device_memory, current_sp, 2)?;
        *self.device_memory.register_mut(gimli::Arm::R3)? =
            read_stack_var(&self.device_memory, current_sp, 3)?;
        *self.device_memory.register_mut(gimli::Arm::R12)? =
            read_stack_var(&self.device_memory, current_sp, 4)?;
        *self.device_memory.register_mut(gimli::Arm::LR)? =
            read_stack_var(&self.device_memory, current_sp, 5)?;
        *self.device_memory.register_mut(gimli::Arm::PC)? =
            read_stack_var(&self.device_memory, current_sp, 6)?;
        // At stack place 7 is the PSR register, but we don't need that, so we skip it

        // Adjust the sp with the size of what we've read
        *self.device_memory.register_mut(gimli::Arm::SP)? =
            self.device_memory.register(gimli::Arm::SP)? + 8 * std::mem::size_of::<u32>() as u32;

        if fpu {
            *self.device_memory.register_mut(gimli::Arm::D0)? =
                read_stack_var(&self.device_memory, current_sp, 8)?;
            *self.device_memory.register_mut(gimli::Arm::D1)? =
                read_stack_var(&self.device_memory, current_sp, 9)?;
            *self.device_memory.register_mut(gimli::Arm::D2)? =
                read_stack_var(&self.device_memory, current_sp, 10)?;
            *self.device_memory.register_mut(gimli::Arm::D3)? =
                read_stack_var(&self.device_memory, current_sp, 11)?;
            *self.device_memory.register_mut(gimli::Arm::D4)? =
                read_stack_var(&self.device_memory, current_sp, 12)?;
            *self.device_memory.register_mut(gimli::Arm::D5)? =
                read_stack_var(&self.device_memory, current_sp, 13)?;
            *self.device_memory.register_mut(gimli::Arm::D6)? =
                read_stack_var(&self.device_memory, current_sp, 14)?;
            *self.device_memory.register_mut(gimli::Arm::D7)? =
                read_stack_var(&self.device_memory, current_sp, 15)?;
            *self.device_memory.register_mut(gimli::Arm::D8)? =
                read_stack_var(&self.device_memory, current_sp, 16)?;
            *self.device_memory.register_mut(gimli::Arm::D9)? =
                read_stack_var(&self.device_memory, current_sp, 17)?;
            *self.device_memory.register_mut(gimli::Arm::D10)? =
                read_stack_var(&self.device_memory, current_sp, 18)?;
            *self.device_memory.register_mut(gimli::Arm::D11)? =
                read_stack_var(&self.device_memory, current_sp, 19)?;
            *self.device_memory.register_mut(gimli::Arm::D12)? =
                read_stack_var(&self.device_memory, current_sp, 20)?;
            *self.device_memory.register_mut(gimli::Arm::D13)? =
                read_stack_var(&self.device_memory, current_sp, 21)?;
            *self.device_memory.register_mut(gimli::Arm::D14)? =
                read_stack_var(&self.device_memory, current_sp, 22)?;
            *self.device_memory.register_mut(gimli::Arm::D15)? =
                read_stack_var(&self.device_memory, current_sp, 23)?;
            // At stack place 24 is the fpscr register, but we don't need that, so we skip it

            // Adjust the sp with the size of what we've read
            *self.device_memory.register_mut(gimli::Arm::SP)? =
                self.device_memory.register(gimli::Arm::SP)?
                    + 17 * std::mem::size_of::<u32>() as u32;
        }

        Ok(())
    }
}

/// Create the stacktrace for the cortex m target.
///
/// - device_memory: All the captured memory of the device.
/// It is not necessary to include any data that is present in the elf file because that will automatically be added.
/// It is required to have a decent chunk of the stack present. If not all of the stack is present,
/// then the eventually the tracing procedure will find a corrupt frame.
/// The standard set of registers is also required to be present.
/// - elf_data: The raw bytes of the elf file.
/// This must be the exact same elf file as the one the device was running. Even a recompilation of the exact same code can change the debug info.
pub fn trace(
    device_memory: DeviceMemory<u32>,
    elf_data: &[u8],
) -> Result<Vec<Frame<u32>>, TraceError> {
    let mut frames = Vec::new();

    let elf = addr2line::object::File::parse(elf_data)?;
    // Get the elf file context
    let mut context = UnwindingContext::create(elf, device_memory)?;

    // Keep looping until we've got the entire trace
    loop {
        context.find_current_frames(&mut frames)?;

        let (unwind_frame, unwinding_left) = context.try_unwind(frames.last_mut())?;

        if let Some(unwind_frame) = unwind_frame {
            frames.push(unwind_frame);
        }

        if !unwinding_left {
            break;
        }
    }

    // Get the static data
    let static_variables = variables::find_static_variables(
        context.addr2line_context.dwarf(),
        &context.device_memory,
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

    Ok(frames)
}
