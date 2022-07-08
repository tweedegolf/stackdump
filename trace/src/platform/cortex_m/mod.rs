//! Trace implementation for the cortex m target

use super::{Platform, UnwindResult};
use crate::error::TraceError;
use crate::{Frame, FrameType};
use addr2line::object::{Object, ObjectSection, ObjectSymbol};
use core::ops::Range;
use gimli::{
    BaseAddresses, CfaRule, DebugFrame, EndianSlice, LittleEndian, RegisterRule, RunTimeEndian,
    UnwindContext, UnwindSection, UnwindTableRow,
};
use stackdump_core::device_memory::DeviceMemory;

const THUMB_BIT: u32 = 1;
const EXC_RETURN_MARKER: u32 = 0xFF00_0000;
const EXC_RETURN_FTYPE_MASK: u32 = 1 << 4;

pub struct CortexMPlatform<'data> {
    debug_frame: DebugFrame<EndianSlice<'data, LittleEndian>>,
    reset_vector_address_range: Range<u32>,
    text_address_range: Range<u32>,
    bases: BaseAddresses,
    unwind_context: UnwindContext<EndianSlice<'data, LittleEndian>>,
}

impl<'data> CortexMPlatform<'data> {
    fn apply_unwind_info(
        device_memory: &mut DeviceMemory<<Self as Platform<'data>>::Word>,
        unwind_info: UnwindTableRow<EndianSlice<LittleEndian>>,
    ) -> Result<bool, TraceError> {
        let updated = match unwind_info.cfa() {
            CfaRule::RegisterAndOffset { register, offset } => {
                let new_cfa = (device_memory.register(*register)? as i64 + *offset) as u32;
                let old_cfa = device_memory.register(gimli::Arm::SP)?;
                let changed = new_cfa != old_cfa;
                *device_memory.register_mut(gimli::Arm::SP)? = new_cfa;
                changed
            }
            CfaRule::Expression(_) => todo!("CfaRule::Expression"),
        };

        for (reg, rule) in unwind_info.registers() {
            match rule {
                RegisterRule::Undefined => unreachable!(),
                RegisterRule::Offset(offset) => {
                    let cfa = device_memory.register(gimli::Arm::SP)?;
                    let addr = (i64::from(cfa) + offset) as u64;
                    let new_value = device_memory
                        .read_u32(addr, RunTimeEndian::Little)?
                        .ok_or(TraceError::MissingMemory(addr))?;
                    *device_memory.register_mut(*reg)? = new_value;
                }
                _ => unimplemented!(),
            }
        }

        Ok(updated)
    }

    fn is_last_frame(
        &self,
        device_memory: &DeviceMemory<<Self as Platform<'data>>::Word>,
    ) -> Result<bool, TraceError> {
        Ok(device_memory.register(gimli::Arm::LR)? == 0
            || (!self
                .text_address_range
                .contains(device_memory.register_ref(gimli::Arm::PC)?)
                && device_memory.register(gimli::Arm::LR)? < EXC_RETURN_MARKER))
    }

    /// Assumes we are at an exception point in the stack unwinding.
    /// Reads the registers that were stored on the stack and updates our current register representation with it.
    ///
    /// Returns Ok if everything went fine or an error with an address if the stack could not be read
    fn update_registers_with_exception_stack(
        device_memory: &mut DeviceMemory<<Self as Platform<'data>>::Word>,
        fpu: bool,
    ) -> Result<(), TraceError> {
        let current_sp = device_memory.register(gimli::Arm::SP)?;

        fn read_stack_var(
            device_memory: &DeviceMemory<u32>,
            starting_sp: u32,
            index: usize,
        ) -> Result<u32, TraceError> {
            device_memory
                .read_u32(starting_sp as u64 + index as u64 * 4, RunTimeEndian::Little)?
                .ok_or(TraceError::MissingMemory(
                    starting_sp as u64 + index as u64 * 4,
                ))
        }

        *device_memory.register_mut(gimli::Arm::R0)? =
            read_stack_var(&device_memory, current_sp, 0)?;
        *device_memory.register_mut(gimli::Arm::R1)? =
            read_stack_var(&device_memory, current_sp, 1)?;
        *device_memory.register_mut(gimli::Arm::R2)? =
            read_stack_var(&device_memory, current_sp, 2)?;
        *device_memory.register_mut(gimli::Arm::R3)? =
            read_stack_var(&device_memory, current_sp, 3)?;
        *device_memory.register_mut(gimli::Arm::R12)? =
            read_stack_var(&device_memory, current_sp, 4)?;
        *device_memory.register_mut(gimli::Arm::LR)? =
            read_stack_var(&device_memory, current_sp, 5)?;
        *device_memory.register_mut(gimli::Arm::PC)? =
            read_stack_var(&device_memory, current_sp, 6)?;
        // At stack place 7 is the PSR register, but we don't need that, so we skip it

        // Adjust the sp with the size of what we've read
        *device_memory.register_mut(gimli::Arm::SP)? = device_memory.register(gimli::Arm::SP)?
            + 8 * std::mem::size_of::<<Self as Platform>::Word>() as <Self as Platform>::Word;

        if fpu {
            *device_memory.register_mut(gimli::Arm::D0)? =
                read_stack_var(&device_memory, current_sp, 8)?;
            *device_memory.register_mut(gimli::Arm::D1)? =
                read_stack_var(&device_memory, current_sp, 9)?;
            *device_memory.register_mut(gimli::Arm::D2)? =
                read_stack_var(&device_memory, current_sp, 10)?;
            *device_memory.register_mut(gimli::Arm::D3)? =
                read_stack_var(&device_memory, current_sp, 11)?;
            *device_memory.register_mut(gimli::Arm::D4)? =
                read_stack_var(&device_memory, current_sp, 12)?;
            *device_memory.register_mut(gimli::Arm::D5)? =
                read_stack_var(&device_memory, current_sp, 13)?;
            *device_memory.register_mut(gimli::Arm::D6)? =
                read_stack_var(&device_memory, current_sp, 14)?;
            *device_memory.register_mut(gimli::Arm::D7)? =
                read_stack_var(&device_memory, current_sp, 15)?;
            *device_memory.register_mut(gimli::Arm::D8)? =
                read_stack_var(&device_memory, current_sp, 16)?;
            *device_memory.register_mut(gimli::Arm::D9)? =
                read_stack_var(&device_memory, current_sp, 17)?;
            *device_memory.register_mut(gimli::Arm::D10)? =
                read_stack_var(&device_memory, current_sp, 18)?;
            *device_memory.register_mut(gimli::Arm::D11)? =
                read_stack_var(&device_memory, current_sp, 19)?;
            *device_memory.register_mut(gimli::Arm::D12)? =
                read_stack_var(&device_memory, current_sp, 20)?;
            *device_memory.register_mut(gimli::Arm::D13)? =
                read_stack_var(&device_memory, current_sp, 21)?;
            *device_memory.register_mut(gimli::Arm::D14)? =
                read_stack_var(&device_memory, current_sp, 22)?;
            *device_memory.register_mut(gimli::Arm::D15)? =
                read_stack_var(&device_memory, current_sp, 23)?;
            // At stack place 24 is the fpscr register, but we don't need that, so we skip it

            // Adjust the sp with the size of what we've read
            *device_memory.register_mut(gimli::Arm::SP)? =
                device_memory.register(gimli::Arm::SP)? + 17 * std::mem::size_of::<u32>() as u32;
        }

        Ok(())
    }
}

impl<'data> Platform<'data> for CortexMPlatform<'data> {
    type Word = u32;

    fn create_context(elf: &addr2line::object::File<'data, &'data [u8]>) -> Result<Self, TraceError>
    where
        Self: Sized,
    {
        let debug_info_sector_data = elf
            .section_by_name(".debug_frame")
            .ok_or_else(|| TraceError::MissingElfSection(".debug_frame".into()))?
            .data()?;
        let mut debug_frame =
            addr2line::gimli::DebugFrame::new(debug_info_sector_data, LittleEndian);
        debug_frame.set_address_size(std::mem::size_of::<Self::Word>() as u8);

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

        Ok(Self {
            debug_frame,
            reset_vector_address_range,
            text_address_range,
            bases,
            unwind_context,
        })
    }

    fn unwind(
        &mut self,
        device_memory: &mut DeviceMemory<Self::Word>,
        previous_frame: Option<&mut Frame<Self::Word>>,
    ) -> Result<super::UnwindResult<Self::Word>, TraceError> {
        let unwind_info = self.debug_frame.unwind_info_for_address(
            &self.bases,
            &mut self.unwind_context,
            device_memory.register(gimli::Arm::PC)? as u64,
            DebugFrame::cie_from_offset,
        );

        let unwind_info = match unwind_info {
            Ok(unwind_info) => unwind_info.clone(),
            Err(_e) => {
                return Ok(UnwindResult::Corrupted {error_frame: Some(Frame { function: "Unknown".into(), location: crate::Location { file: None, line: None, column: None }, frame_type: FrameType::Corrupted(format!("debug information for address {:#x} is missing. Likely fixes:
                1. compile the Rust code with `debug = 1` or higher. This is configured in the `profile.{{release,bench}}` sections of Cargo.toml (`profile.{{dev,test}}` default to `debug = 2`)
                2. use a recent version of the `cortex-m` crates (e.g. cortex-m 0.6.3 or newer). Check versions in Cargo.lock
                3. if linking to C code, compile the C code with the `-g` flag", device_memory.register(gimli::Arm::PC)?)),
                    variables: Vec::new(), }) });
            }
        };

        // We can update the stackpointer and other registers to the previous frame by applying the unwind info
        let stack_pointer_changed = match Self::apply_unwind_info(device_memory, unwind_info) {
            Ok(stack_pointer_changed) => stack_pointer_changed,
            Err(e) => {
                return Ok(UnwindResult::Corrupted {
                    error_frame: Some(Frame {
                        function: "Unknown".into(),
                        location: crate::Location {
                            file: None,
                            line: None,
                            column: None,
                        },
                        frame_type: FrameType::Corrupted(e.to_string()),
                        variables: Vec::new(),
                    }),
                });
            }
        };

        // We're not at the last frame. What's the reason?

        // Do we have a corrupted stack?
        if !stack_pointer_changed
            && device_memory.register(gimli::Arm::LR)? & !THUMB_BIT
                == device_memory.register(gimli::Arm::PC)? & !THUMB_BIT
        {
            // The stack pointer didn't change and our LR points to our current PC
            // If we unwound further we'd get the same frame again so we better stop

            return Ok(UnwindResult::Corrupted {
                error_frame: Some(Frame {
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
            });
        }

        // Stack is not corrupted, but unwinding is not done
        // Are we returning from an exception? (EXC_RETURN)
        if device_memory.register(gimli::Arm::LR)? >= EXC_RETURN_MARKER {
            // Yes, so the registers were pushed to the stack and we need to get them back

            // Check the value to know if there are fpu registers to read
            let fpu = device_memory.register(gimli::Arm::LR)? & EXC_RETURN_FTYPE_MASK > 0;

            if let Some(previous_frame) = previous_frame {
                previous_frame.frame_type = FrameType::Exception;
            }

            match Self::update_registers_with_exception_stack(device_memory, fpu) {
                Ok(()) => {}
                Err(TraceError::MissingMemory(address)) => {
                    return Ok(UnwindResult::Corrupted {
                        error_frame: Some(Frame {
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
                    });
                }
                Err(e) => return Err(e),
            }
        } else {
            // No exception, so follow the LR back
            *device_memory.register_mut(gimli::Arm::PC)? = device_memory.register(gimli::Arm::LR)?
        }

        // Have we reached the reset vector?
        if self
            .reset_vector_address_range
            .contains(device_memory.register_ref(gimli::Arm::PC)?)
        {
            // Yes, let's make that a frame as well
            // We'll also make an assumption that there's no frames before reset
            return Ok(UnwindResult::Finished);
        }

        if self.is_last_frame(device_memory)? {
            Ok(UnwindResult::Finished)
        } else {
            // Is our stack pointer in a weird place?
            if device_memory
                .read_u32(
                    device_memory.register(gimli::Arm::SP)? as u64,
                    RunTimeEndian::Little,
                )?
                .is_none()
            {
                Ok(UnwindResult::Corrupted {error_frame:Some(Frame {
                    function: "Unknown".into(),
                    location: crate::Location { file: None, line: None, column: None },
                    frame_type: FrameType::Corrupted(
                        format!("The stack pointer ({:#08X}) is corrupted or the dump does not contain the full stack", device_memory
                        .register(gimli::Arm::SP)?),
                    ),
                    variables: Vec::new(),
                })})
            } else {
                Ok(UnwindResult::Proceeded)
            }
        }
    }
}
