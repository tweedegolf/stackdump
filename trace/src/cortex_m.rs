use std::{error::Error, ops::Range};

use addr2line::{
    object::{File, Object, ObjectSection, ObjectSymbol},
    Context,
};
use gimli::{
    BaseAddresses, CfaRule, DebugFrame, EndianRcSlice, EndianSlice, LittleEndian, RegisterRule,
    RunTimeEndian, UnwindContext, UnwindSection, UnwindTableRow,
};
use stackdump_capture::cortex_m::{CortexMRegisters, CortexMTarget};
use stackdump_core::Stackdump;

use crate::{Frame, FrameType, Trace};

struct UnwindingContext<'data, F>
where
    F: Fn(u32) -> Option<u32>,
{
    debug_frame: DebugFrame<EndianSlice<'data, LittleEndian>>,
    reset_vector_address_range: Range<u32>,
    text_address_range: Range<u32>,
    addr2line_context: Context<EndianRcSlice<RunTimeEndian>>,
    registers: CortexMRegisters,
    stack_reader: F,
    bases: BaseAddresses,
    unwind_context: UnwindContext<EndianSlice<'data, LittleEndian>>,
}

impl<'data, F> UnwindingContext<'data, F>
where
    F: Fn(u32) -> Option<u32>,
{
    const THUMB_BIT: u32 = 1;
    const LR_END: u32 = 0xFFFF_FFFF;

    pub fn create(
        elf: File<'data>,
        registers: CortexMRegisters,
        stack_reader: F,
    ) -> Result<Self, Box<dyn Error>> {
        let addr2line_context = addr2line::Context::new(&elf).unwrap();

        let mut debug_frame = addr2line::gimli::DebugFrame::new(
            elf.section_by_name(".debug_frame")
                .ok_or("Could not find .debug_frame section")?
                .data()?,
            LittleEndian,
        );
        debug_frame.set_address_size(std::mem::size_of::<u32>() as u8);

        let vector_table_section = elf
            .section_by_name(".vector_table")
            .ok_or("Could not find .vector_table section")?;
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
            .ok_or("Could not find .text section")?;
        let text_address_range = (text_section.address() as u32)
            ..(text_section.address() as u32 + text_section.size() as u32);

        let bases = BaseAddresses::default();
        let unwind_context = UnwindContext::new();

        Ok(Self {
            debug_frame,
            reset_vector_address_range,
            text_address_range,
            addr2line_context,
            registers,
            stack_reader,
            bases,
            unwind_context,
        })
    }

    pub fn find_current_frames(&mut self, frames: &mut Vec<Frame>) -> Result<(), Box<dyn Error>> {
        // Find the frames of the current register context
        let mut context_frames = self
            .addr2line_context
            .find_frames(*self.registers.base.pc() as u64)
            .unwrap();

        // Loop through the found frames and add them
        let mut added_frames = 0;
        while let Some(context_frame) = context_frames.next()? {
            let (file, line, column) = context_frame
                .location
                .map(|l| (l.file.map(|f| f.to_string()), l.line, l.column))
                .unwrap_or_default();

            frames.push(Frame {
                function: context_frame
                    .function
                    .map(|f| f.demangle().ok().map(|f| f.into_owned()))
                    .flatten(),
                file,
                line,
                column,
                frame_type: FrameType::InlineFunction,
            });

            added_frames += 1;
        }

        if added_frames > 0 {
            // The last frame of `find_frames` is always a real function. All frames before are inline functions.
            frames.last_mut().unwrap().frame_type = FrameType::Function;
        }

        Ok(())
    }

    pub fn try_unwind(&mut self, frames: &mut Vec<Frame>) -> Result<bool, Box<dyn Error>> {
        let unwind_info = self.debug_frame.unwind_info_for_address(
            &self.bases,
            &mut self.unwind_context,
            *self.registers.base.pc() as u64,
            DebugFrame::cie_from_offset,
        );

        let unwind_info = match unwind_info {
            Ok(unwind_info) => unwind_info.clone(),
            Err(_e) => {
                frames.push(Frame { function: Some("Unknown".into()), file: None, line: None, column: None, frame_type: FrameType::Corrupted(format!("debug information for address {:#x} is missing. Likely fixes:
                1. compile the Rust code with `debug = 1` or higher. This is configured in the `profile.{{release,bench}}` sections of Cargo.toml (`profile.{{dev,test}}` default to `debug = 2`)
                2. use a recent version of the `cortex-m` crates (e.g. cortex-m 0.6.3 or newer). Check versions in Cargo.lock
                3. if linking to C code, compile the C code with the `-g` flag", self.registers.base.pc())) });
                return Ok(false);
            }
        };

        // We can update the stackpointer and other registers to the previous frame by applying the unwind info
        let stack_pointer_changed = match self.apply_unwind_info(unwind_info) {
            Ok(stack_pointer_changed) => stack_pointer_changed,
            Err(e) => {
                frames.push(Frame {
                    function: Some("Unknown".into()),
                    file: None,
                    line: None,
                    column: None,
                    frame_type: FrameType::Corrupted(e.to_string()),
                });
                return Ok(false);
            }
        };

        // We're not at the last frame. What's the reason?

        // Do we have a corrupted stack?
        if !stack_pointer_changed
            && *self.registers.base.lr() & !Self::THUMB_BIT
                == *self.registers.base.pc() & !Self::THUMB_BIT
        {
            // The stack pointer didn't change and our LR points to our current PC
            // If we unwound further we'd get the same frame again so we better stop

            frames.push(Frame {
                function: Some("Unknown".into()),
                file: None,
                line: None,
                column: None,
                frame_type: FrameType::Corrupted(
                    "CFA did not change and LR and PC are equal".into(),
                ),
            });
            return Ok(false);
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
                    return Err(format!(
                        "LR contains invalid EXC_RETURN value 0x{:08X}",
                        *self.registers.base.lr()
                    )
                    .into())
                }
            };

            if let Some(last_frame) = frames.last_mut() {
                last_frame.frame_type = FrameType::Exception;
            }

            self.update_registers_with_exception_stack(fpu)?;
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
            frames.push(Frame { function: Some("RESET".into()), file: None, line: None, column: None, frame_type: FrameType::Function })
        }

        Ok(!self.is_last_frame())
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
                    let new_value = (self.stack_reader)(addr)
                        .ok_or(format!("Address {:#010X} not within stack space", addr))?;
                    *self.registers.base.register_mut(reg.0 as usize) = new_value;
                }
                _ => unimplemented!(),
            }
        }

        Ok(updated)
    }

    fn is_last_frame(&self) -> bool {
        *self.registers.base.lr() == Self::LR_END
            || *self.registers.base.lr() == 0
            || self
                .reset_vector_address_range
                .contains(self.registers.base.pc())
            || (!self.text_address_range.contains(self.registers.base.pc())
                && *self.registers.base.lr() <= 0xFFFF_FFE0)
    }

    fn update_registers_with_exception_stack(&mut self, fpu: bool) -> Result<(), Box<dyn Error>> {
        let current_sp = *self.registers.base.sp();

        let read_stack_var = |index: u32| {
            (self.stack_reader)(current_sp + index * 4).ok_or(format!(
                "Address {:#10X} out of range",
                current_sp + index * 4
            ))
        };
        *self.registers.base.register_mut(0) = read_stack_var(0)?;
        *self.registers.base.register_mut(1) = read_stack_var(1)?;
        *self.registers.base.register_mut(2) = read_stack_var(2)?;
        *self.registers.base.register_mut(3) = read_stack_var(3)?;
        *self.registers.base.register_mut(12) = read_stack_var(4)?;
        *self.registers.base.lr_mut() = read_stack_var(5)?;
        *self.registers.base.pc_mut() = read_stack_var(6)?;
        *self.registers.base.psr_mut() = read_stack_var(7)?;
        // Adjust the sp with the size of what we've read
        *self.registers.base.sp_mut() = *self.registers.base.sp() + 8;

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
            *self.registers.fpu.fpscr_mut() = read_stack_var(24)?;
            // Adjust the sp with the size of what we've read
            *self.registers.base.sp_mut() = *self.registers.base.sp() + 17;
        }

        Ok(())
    }
}

impl<const STACK_SIZE: usize> Trace for Stackdump<CortexMTarget, STACK_SIZE> {
    fn trace(&self, elf_data: &[u8]) -> Result<Vec<crate::Frame>, Box<dyn Error>> {
        let mut frames = Vec::new();

        // Get the elf file context
        let elf = addr2line::object::File::parse(elf_data).unwrap();
        let mut context = UnwindingContext::create(elf, self.registers.clone(), |address| {
            let start_address = *self.registers.base.sp();
            let relative_address = address.checked_sub(start_address)? as usize;

            if self.stack.len() >= relative_address + 4 {
                Some(u32::from_le_bytes(
                    self.stack[relative_address..(relative_address + 4)]
                        .try_into()
                        .unwrap(),
                ))
            } else {
                None
            }
        })?;

        // Keep looping until we've got the entire trace
        loop {
            context.find_current_frames(&mut frames)?;
            if !context.try_unwind(&mut frames)? {
                break;
            }
        }

        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELF: &[u8] = include_bytes!("../../examples/data/nrf52840");
    const DUMP: &[u8] = include_bytes!("../../examples/data/nrf52840.dump");

    #[test]
    fn example_dump() {
        let stackdump: Stackdump<CortexMTarget, 32768> = serde_json::from_slice(DUMP).unwrap();
        let frames = stackdump.trace(ELF).unwrap();
        for (i, frame) in frames.iter().enumerate() {
            println!("{}: {}", i, frame);
        }
    }
}
