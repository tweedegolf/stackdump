use crate::{Frame, FrameType, Trace};
use addr2line::{
    object::{self, File, Object, ObjectSection, ObjectSymbol, SectionKind},
    Context,
};
use gimli::{
    BaseAddresses, CfaRule, DebugFrame, EndianRcSlice, EndianSlice, LittleEndian, RegisterRule,
    RunTimeEndian, UnwindContext, UnwindSection, UnwindTableRow,
};
use stackdump_capture::cortex_m::{CortexMRegisters, CortexMTarget};
use stackdump_core::{
    device_memory::DeviceMemory,
    memory_region::{MemoryRegion, VecMemoryRegion},
    Stackdump,
};
use std::{error::Error, ops::Range};
use thiserror::Error;

mod variables;

#[derive(Error, Debug)]
pub enum TraceError {
    #[error("The elf file does not contain the required `{0}` section")]
    MissingElfSection(String),
    #[error("The elf file could not be read: {0}")]
    ObjectReadError(#[from] object::Error),
    #[error("An IO error occured: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Some debug information could not be parsed: {0}")]
    DebugParseError(#[from] gimli::Error),
    #[error("An entry ({entry_tag} (@ .debug_info offset {entry_debug_info_offset:X?})) is missing an expected attribute: {attribute_name}")]
    MissingAttribute {
        entry_debug_info_offset: Option<u64>,
        entry_tag: String,
        attribute_name: String,
    },
    #[error("An attribute ({attribute_name}) has the wrong value type: {value_type_name}")]
    WrongAttributeValueType {
        attribute_name: String,
        value_type_name: &'static str,
    },
    #[error("The type `{type_name}` has not been implemented yet")]
    TypeNotImplemented { type_name: String },
    #[error("A child was expected for {entry_tag}, but it was not there")]
    ExpectedChildNotPresent { entry_tag: String },
    #[error("The frame base is not known yet")]
    UnknownFrameBase,
    #[error("The dwarf unit for a `pc` of {pc:#X} could not be found")]
    DwarfUnitNotFound { pc: u64 },
    #[error("A number could not be converted to another type")]
    NumberConversionError,
}

pub(self) const THUMB_BIT: u32 = 1;

struct UnwindingContext<'data> {
    debug_frame: DebugFrame<EndianSlice<'data, LittleEndian>>,
    reset_vector_address_range: Range<u32>,
    text_address_range: Range<u32>,
    addr2line_context: Context<EndianRcSlice<RunTimeEndian>>,
    registers: CortexMRegisters,
    device_memory: DeviceMemory,
    bases: BaseAddresses,
    unwind_context: UnwindContext<EndianSlice<'data, LittleEndian>>,
}

impl<'data> UnwindingContext<'data> {
    pub fn create(
        elf: File<'data>,
        registers: CortexMRegisters,
        stack: Box<dyn MemoryRegion>,
    ) -> Result<Self, TraceError> {
        let addr2line_context = addr2line::Context::new(&elf)?;

        let debug_info_sector_data = elf
            .section_by_name(".debug_frame")
            .ok_or(TraceError::MissingElfSection(".debug_frame".into()))?
            .data()?;
        let mut debug_frame =
            addr2line::gimli::DebugFrame::new(debug_info_sector_data, LittleEndian);
        debug_frame.set_address_size(std::mem::size_of::<u32>() as u8);

        let vector_table_section = elf
            .section_by_name(".vector_table")
            .ok_or(TraceError::MissingElfSection(".vector_table".into()))?;
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
            .ok_or(TraceError::MissingElfSection(".text".into()))?;
        let text_address_range = (text_section.address() as u32)
            ..(text_section.address() as u32 + text_section.size() as u32);

        let bases = BaseAddresses::default();
        let unwind_context = UnwindContext::new();

        let mut device_memory = DeviceMemory::new();
        device_memory.add_memory_region_boxed(stack);

        for section in elf.sections().filter(|section| match section.kind() {
            SectionKind::Text | SectionKind::ReadOnlyData | SectionKind::ReadOnlyString => true,
            _ => false,
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
            registers,
            device_memory,
            bases,
            unwind_context,
        })
    }

    pub fn find_current_frames(&mut self, frames: &mut Vec<Frame>) -> Result<(), TraceError> {
        // Find the frames of the current register context
        let mut context_frames = self
            .addr2line_context
            .find_frames(*self.registers.base.pc() as u64)?;

        // Get the debug compilation unit of the current register context
        let unit = self
            .addr2line_context
            .find_dwarf_unit(*self.registers.base.pc() as u64)
            .ok_or_else(|| TraceError::DwarfUnitNotFound {
                pc: *self.registers.base.pc() as u64,
            })?;

        // Get the abbreviations of the unit
        let abbreviations = self.addr2line_context.dwarf().abbreviations(&unit.header)?;

        // Loop through the found frames and add them
        let mut added_frames = 0;
        while let Some(context_frame) = context_frames.next()? {
            let (file, line, column) = context_frame
                .location
                .map(|l| (l.file.map(|f| f.to_string()), l.line, l.column))
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
                    variables::find_variables(
                        &self.addr2line_context,
                        unit,
                        &abbreviations,
                        &self.registers,
                        &self.device_memory,
                        entry_root,
                        &mut variables,
                        None,
                    )?;
                }
            }

            frames.push(Frame {
                function: context_frame
                    .function
                    .map(|f| f.demangle().ok().map(|f| f.into_owned()))
                    .flatten(),
                file,
                line,
                column,
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
    pub fn try_unwind(&mut self, last_frame: Option<&mut Frame>) -> (Option<Frame>, bool) {
        let unwind_info = self.debug_frame.unwind_info_for_address(
            &self.bases,
            &mut self.unwind_context,
            *self.registers.base.pc() as u64,
            DebugFrame::cie_from_offset,
        );

        let unwind_info = match unwind_info {
            Ok(unwind_info) => unwind_info.clone(),
            Err(_e) => {
                return (Some(Frame { function: Some("Unknown".into()), file: None, line: None, column: None, frame_type: FrameType::Corrupted(format!("debug information for address {:#x} is missing. Likely fixes:
                1. compile the Rust code with `debug = 1` or higher. This is configured in the `profile.{{release,bench}}` sections of Cargo.toml (`profile.{{dev,test}}` default to `debug = 2`)
                2. use a recent version of the `cortex-m` crates (e.g. cortex-m 0.6.3 or newer). Check versions in Cargo.lock
                3. if linking to C code, compile the C code with the `-g` flag", self.registers.base.pc())),
                    variables: Vec::new(), }), false);
            }
        };

        // We can update the stackpointer and other registers to the previous frame by applying the unwind info
        let stack_pointer_changed = match self.apply_unwind_info(unwind_info) {
            Ok(stack_pointer_changed) => stack_pointer_changed,
            Err(e) => {
                return (
                    Some(Frame {
                        function: Some("Unknown".into()),
                        file: None,
                        line: None,
                        column: None,
                        frame_type: FrameType::Corrupted(e.to_string()),
                        variables: Vec::new(),
                    }),
                    false,
                );
            }
        };

        // We're not at the last frame. What's the reason?

        // Do we have a corrupted stack?
        if !stack_pointer_changed
            && *self.registers.base.lr() & !THUMB_BIT == *self.registers.base.pc() & !THUMB_BIT
        {
            // The stack pointer didn't change and our LR points to our current PC
            // If we unwound further we'd get the same frame again so we better stop

            return (
                Some(Frame {
                    function: Some("Unknown".into()),
                    file: None,
                    line: None,
                    column: None,
                    frame_type: FrameType::Corrupted(
                        "CFA did not change and LR and PC are equal".into(),
                    ),
                    variables: Vec::new(),
                }),
                false,
            );
        }

        // Stack is not corrupted, but unwinding is not done
        // Are we returning from an exception? (EXC_RETURN)
        if *self.registers.base.lr() > 0xffff_ffe0 {
            // Yes, so the registers were pushed to the stack and we need to get them back

            // Check the value to know if there are fpu registers to read
            let fpu = match *self.registers.base.lr() {
                0xFFFFFFF1 | 0xFFFFFFF9 | 0xFFFFFFFD => false,
                0xFFFFFFE1 | 0xFFFFFFE9 | 0xFFFFFFED => true,
                _ => {
                    return (
                        Some(Frame {
                            function: Some("Unknown".into()),
                            file: None,
                            line: None,
                            column: None,
                            frame_type: FrameType::Corrupted(format!(
                                "LR contains invalid EXC_RETURN value {:#10X}",
                                *self.registers.base.lr()
                            )),
                            variables: Vec::new(),
                        }),
                        false,
                    );
                }
            };

            if let Some(last_frame) = last_frame {
                last_frame.frame_type = FrameType::Exception;
            }

            match self.update_registers_with_exception_stack(fpu) {
                Ok(()) => {}
                Err(address) => {
                    return (
                        Some(Frame {
                            function: Some("Unknown".into()),
                            file: None,
                            line: None,
                            column: None,
                            frame_type: FrameType::Corrupted(format!(
                                "Could not read address {:#10X} from the stack",
                                address
                            )),
                            variables: Vec::new(),
                        }),
                        false,
                    );
                }
            }
        } else {
            // No exception, so follow the LR back
            *self.registers.base.pc_mut() = *self.registers.base.lr();
        }

        // Have we reached the reset vector?
        if self
            .reset_vector_address_range
            .contains(self.registers.base.pc())
        {
            // Yes, let's make that a frame as well
            // We'll also make an assumption that there's no frames before reset
            return (
                Some(Frame {
                    function: Some("RESET".into()),
                    file: None,
                    line: None,
                    column: None,
                    frame_type: FrameType::Function,
                    variables: Vec::new(),
                }),
                false,
            );
        }

        if self.is_last_frame() {
            (None, false)
        } else {
            // Is our stack pointer in a weird place?
            if self
                .device_memory
                .read_u32(*self.registers.base.sp() as usize, RunTimeEndian::Little)
                .is_none()
            {
                (Some(Frame {
                    function: Some("Unknown".into()),
                    file: None,
                    line: None,
                    column: None,
                    frame_type: FrameType::Corrupted(
                        format!("The stack pointer ({:#08X}) is corrupted or the dump does not contain the full stack", *self.registers.base.sp()),
                    ),
                    variables: Vec::new(),
                }), false)
            } else {
                (None, true)
            }
        }
    }

    fn apply_unwind_info(
        &mut self,
        unwind_info: UnwindTableRow<EndianSlice<LittleEndian>>,
    ) -> Result<bool, Box<dyn Error>> {
        let updated = match unwind_info.cfa() {
            CfaRule::RegisterAndOffset { register, offset } => {
                let new_cfa =
                    (i64::from(*self.registers.base.register(register.0 as usize)) + offset) as u32;
                let old_cfa = *self.registers.base.sp();
                let changed = new_cfa != old_cfa;
                *self.registers.base.sp_mut() = new_cfa;
                changed
            }
            CfaRule::Expression(_) => todo!("CfaRule::Expression"),
        };

        for (reg, rule) in unwind_info.registers() {
            match rule {
                RegisterRule::Undefined => unreachable!(),
                RegisterRule::Offset(offset) => {
                    let cfa = *self.registers.base.sp();
                    let addr = (i64::from(cfa) + offset) as u32;
                    let new_value = self
                        .device_memory
                        .read_u32(addr as usize, RunTimeEndian::Little)
                        .ok_or(format!("Address {:#010X} not within stack space", addr))?;
                    *self.registers.base.register_mut(reg.0 as usize) = new_value;
                }
                _ => unimplemented!(),
            }
        }

        Ok(updated)
    }

    fn is_last_frame(&self) -> bool {
        *self.registers.base.lr() == 0
            || (!self.text_address_range.contains(self.registers.base.pc())
                && *self.registers.base.lr() <= 0xFFFF_FFE0)
    }

    /// Assumes we are at an exception point in the stack unwinding.
    /// Reads the registers that were stored on the stack and updates our current register representation with it.
    ///
    /// Returns Ok if everything went fine or an error with an address if the stack could not be read
    fn update_registers_with_exception_stack(&mut self, fpu: bool) -> Result<(), usize> {
        let current_sp = *self.registers.base.sp();

        let read_stack_var = |index: usize| {
            self.device_memory
                .read_u32(current_sp as usize + index * 4, RunTimeEndian::Little)
                .ok_or(current_sp as usize + index * 4)
        };
        *self.registers.base.register_mut(0) = read_stack_var(0)?;
        *self.registers.base.register_mut(1) = read_stack_var(1)?;
        *self.registers.base.register_mut(2) = read_stack_var(2)?;
        *self.registers.base.register_mut(3) = read_stack_var(3)?;
        *self.registers.base.register_mut(12) = read_stack_var(4)?;
        *self.registers.base.lr_mut() = read_stack_var(5)?;
        *self.registers.base.pc_mut() = read_stack_var(6)?;
        // Adjust the sp with the size of what we've read
        *self.registers.base.sp_mut() =
            *self.registers.base.sp() + 8 * std::mem::size_of::<u32>() as u32;

        if fpu {
            *self.registers.fpu.fpu_register_mut(0) = read_stack_var(8)?;
            *self.registers.fpu.fpu_register_mut(1) = read_stack_var(9)?;
            *self.registers.fpu.fpu_register_mut(2) = read_stack_var(10)?;
            *self.registers.fpu.fpu_register_mut(3) = read_stack_var(11)?;
            *self.registers.fpu.fpu_register_mut(4) = read_stack_var(12)?;
            *self.registers.fpu.fpu_register_mut(5) = read_stack_var(13)?;
            *self.registers.fpu.fpu_register_mut(6) = read_stack_var(14)?;
            *self.registers.fpu.fpu_register_mut(7) = read_stack_var(15)?;
            *self.registers.fpu.fpu_register_mut(8) = read_stack_var(16)?;
            *self.registers.fpu.fpu_register_mut(9) = read_stack_var(17)?;
            *self.registers.fpu.fpu_register_mut(10) = read_stack_var(18)?;
            *self.registers.fpu.fpu_register_mut(11) = read_stack_var(19)?;
            *self.registers.fpu.fpu_register_mut(12) = read_stack_var(20)?;
            *self.registers.fpu.fpu_register_mut(13) = read_stack_var(21)?;
            *self.registers.fpu.fpu_register_mut(14) = read_stack_var(22)?;
            *self.registers.fpu.fpu_register_mut(15) = read_stack_var(23)?;
            // Adjust the sp with the size of what we've read
            *self.registers.base.sp_mut() = *self.registers.base.sp() + 17;
        }

        Ok(())
    }
}

impl<const STACK_SIZE: usize> Trace for Stackdump<CortexMTarget, STACK_SIZE> {
    type Error = TraceError;

    fn trace(&self, elf_data: &[u8]) -> Result<Vec<crate::Frame>, Self::Error> {
        let mut frames = Vec::new();

        let elf = addr2line::object::File::parse(elf_data)?;
        // Get the elf file context
        let mut context =
            UnwindingContext::create(elf, self.registers.clone(), Box::new(self.stack.clone()))?;

        // Keep looping until we've got the entire trace
        loop {
            // #[cfg(test)]
            // println!("{:02X?}", context.registers);

            context.find_current_frames(&mut frames)?;

            let (unwind_frame, unwinding_left) = context.try_unwind(frames.last_mut());

            if let Some(unwind_frame) = unwind_frame {
                frames.push(unwind_frame);
            }

            if !unwinding_left {
                break;
            }
        }

        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELF: &[u8] = include_bytes!("../../../examples/data/nrf52840");
    const DUMP: &[u8] = include_bytes!("../../../examples/data/nrf52840.dump");

    #[test]
    fn example_dump() {
        simple_logger::SimpleLogger::new().with_level(log::LevelFilter::Debug).init().unwrap();

        let stackdump: Stackdump<CortexMTarget, 2048> = Stackdump::try_from(DUMP).unwrap();
        let frames = stackdump.trace(ELF).unwrap();
        for (i, frame) in frames.iter().enumerate() {
            println!("{}: {}", i, frame);
        }
    }
}
