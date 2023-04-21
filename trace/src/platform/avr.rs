use std::ops::Range;

use addr2line::object::{Object, ObjectSection, ObjectSymbol};
use gimli::{BaseAddresses, UnwindContext, LittleEndian, EndianSlice, UnwindTableRow, DebugFrame, RunTimeEndian, UnwindSection, RegisterRule, CfaRule};
use stackdump_core::device_memory::DeviceMemory;

use crate::{error::TraceError, Frame, FrameType};

use super::{Platform, UnwindResult};

pub struct AvrPlatform {
    text_address_range: Range<u16>,
}

impl AvrPlatform {
    const SP: gimli::Register = gimli::Register(32);
    const PC: gimli::Register = gimli::Register(33);
}

impl<'data> Platform<'data> for AvrPlatform {
    type Word = u16;

    fn create_context(elf: &addr2line::object::File<'data, &'data [u8]>) -> Result<Self, TraceError>
    where
        Self: Sized,
    {
        let text_section = elf
            .section_by_name(".text")
            .ok_or_else(|| TraceError::MissingElfSection(".text".into()))?;
        let text_address_range = (text_section.address() as u16)
            ..(text_section.address() as u16 + text_section.size() as u16);


        Ok(Self {
            text_address_range,
        })
    }

    fn unwind(
        &mut self,
        device_memory: &mut DeviceMemory<Self::Word>,
        previous_frame: Option<&mut Frame<Self::Word>>,
    ) -> Result<super::UnwindResult<Self::Word>, TraceError> {
        Ok(UnwindResult::Finished)
    }
}
