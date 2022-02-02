use gimli::{
    Attribute, AttributeValue, DebugStr, DebuggingInformationEntry, DwAt, Expression, Reader,
    ReaderOffset, Unit,
};

use crate::cortex_m::TraceError;

pub trait DebuggingInformationEntryExt<R: Reader> {
    fn required_attr(
        &self,
        unit: &Unit<R, R::Offset>,
        name: DwAt,
    ) -> Result<Attribute<R>, TraceError>;
}

impl<R, O> DebuggingInformationEntryExt<R> for DebuggingInformationEntry<'_, '_, R>
where
    O: ReaderOffset + TryInto<u64>,
    R: Reader<Offset = O>,
{
    fn required_attr(&self, unit: &Unit<R, O>, name: DwAt) -> Result<Attribute<R>, TraceError> {
        if let Some(attr) = self.attr(name)? {
            Ok(attr)
        } else {
            let unit_section_offset = match unit.header.offset() {
                gimli::UnitSectionOffset::DebugInfoOffset(o) => Some(
                    o.0.try_into()
                        .map_err(|_| TraceError::NumberConversionError)?,
                ),
                gimli::UnitSectionOffset::DebugTypesOffset(_) => None,
            };

            let unit_offset: u64 = self
                .offset()
                .0
                .try_into()
                .map_err(|_| TraceError::NumberConversionError)?;

            Err(TraceError::MissingAttribute {
                entry_debug_info_offset: unit_section_offset.map(|uo| uo + unit_offset),
                entry_tag: self.tag().to_string(),
                attribute_name: name.to_string(),
            })
        }
    }
}

impl<R, E> DebuggingInformationEntryExt<R> for &E
where
    R: Reader,
    E: DebuggingInformationEntryExt<R>,
{
    fn required_attr(
        &self,
        unit: &Unit<R, R::Offset>,
        name: DwAt,
    ) -> Result<Attribute<R>, TraceError> {
        (*self).required_attr(unit, name)
    }
}

pub trait AttributeExt<R: Reader> {
    fn required_u8_value(&self) -> Result<u8, TraceError>;
    fn required_u16_value(&self) -> Result<u16, TraceError>;
    fn required_udata_value(&self) -> Result<u64, TraceError>;
    fn required_sdata_value(&self) -> Result<i64, TraceError>;
    fn required_offset_value(&self) -> Result<R::Offset, TraceError>;
    fn required_exprloc_value(&self) -> Result<Expression<R>, TraceError>;
    fn required_string_value(&self, debug_str: &DebugStr<R>) -> Result<R, TraceError>;
    fn required_string_value_sup(
        &self,
        debug_str: &DebugStr<R>,
        debug_str_sup: Option<&DebugStr<R>>,
    ) -> Result<R, TraceError>;
}

impl<R: Reader> AttributeExt<R> for Attribute<R> {
    fn required_u8_value(&self) -> Result<u8, TraceError> {
        self.u8_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_u16_value(&self) -> Result<u16, TraceError> {
        self.u16_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_udata_value(&self) -> Result<u64, TraceError> {
        self.udata_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_sdata_value(&self) -> Result<i64, TraceError> {
        self.sdata_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_offset_value(&self) -> Result<R::Offset, TraceError> {
        self.offset_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_exprloc_value(&self) -> Result<Expression<R>, TraceError> {
        self.exprloc_value()
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_string_value(&self, debug_str: &DebugStr<R>) -> Result<R, TraceError> {
        self.string_value(debug_str)
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }

    fn required_string_value_sup(
        &self,
        debug_str: &DebugStr<R>,
        debug_str_sup: Option<&DebugStr<R>>,
    ) -> Result<R, TraceError> {
        self.string_value_sup(debug_str, debug_str_sup)
            .ok_or_else(|| TraceError::WrongAttributeValueType {
                attribute_name: self.name().to_string(),
                value_type_name: get_attribute_value_type_name(&self.value()),
            })
    }
}

fn get_attribute_value_type_name<R: Reader>(attribute_value: &AttributeValue<R>) -> &'static str {
    match attribute_value {
        AttributeValue::Block(_) => "Block",
        AttributeValue::Data1(_) => "Data1",
        AttributeValue::Data2(_) => "Data2",
        AttributeValue::Data4(_) => "Data4",
        AttributeValue::Data8(_) => "Data8",
        AttributeValue::Sdata(_) => "Sdata",
        AttributeValue::Udata(_) => "Udata",
        AttributeValue::Exprloc(_) => "Exprloc",
        AttributeValue::Flag(_) => "Flag",
        AttributeValue::UnitRef(_) => "UnitRef",
        AttributeValue::DebugInfoRef(_) => "DebugInfoRef",
        AttributeValue::DebugInfoRefSup(_) => "DebugInfoRefSup",
        AttributeValue::DebugMacinfoRef(_) => "DebugMacinfoRef",
        AttributeValue::DebugMacroRef(_) => "DebugMacroRef",
        AttributeValue::DebugTypesRef(_) => "DebugTypesRef",
        AttributeValue::DebugStrRefSup(_) => "DebugStrRefSup",
        AttributeValue::String(_) => "String",
        AttributeValue::Encoding(_) => "Encoding",
        AttributeValue::DecimalSign(_) => "DecimalSign",
        AttributeValue::Endianity(_) => "Endianity",
        AttributeValue::Accessibility(_) => "Accessibility",
        AttributeValue::Visibility(_) => "Visibility",
        AttributeValue::Virtuality(_) => "Virtuality",
        AttributeValue::Language(_) => "Language",
        AttributeValue::AddressClass(_) => "AddressClass",
        AttributeValue::IdentifierCase(_) => "IdentifierCase",
        AttributeValue::CallingConvention(_) => "CallingConvention",
        AttributeValue::Inline(_) => "Inline",
        AttributeValue::Ordering(_) => "Ordering",
        AttributeValue::FileIndex(_) => "FileIndex",
        AttributeValue::Addr(_) => "Addr",
        AttributeValue::SecOffset(_) => "SecOffset",
        AttributeValue::DebugAddrBase(_) => "DebugAddrBase",
        AttributeValue::DebugAddrIndex(_) => "DebugAddrIndex",
        AttributeValue::DebugLineRef(_) => "DebugLineRef",
        AttributeValue::LocationListsRef(_) => "LocationListsRef",
        AttributeValue::DebugLocListsBase(_) => "DebugLocListsBase",
        AttributeValue::DebugLocListsIndex(_) => "DebugLocListsIndex",
        AttributeValue::RangeListsRef(_) => "RangeListsRef",
        AttributeValue::DebugRngListsBase(_) => "DebugRngListsBase",
        AttributeValue::DebugRngListsIndex(_) => "DebugRngListsIndex",
        AttributeValue::DebugStrRef(_) => "DebugStrRef",
        AttributeValue::DebugStrOffsetsBase(_) => "DebugStrOffsetsBase",
        AttributeValue::DebugStrOffsetsIndex(_) => "DebugStrOffsetsIndex",
        AttributeValue::DebugLineStrRef(_) => "DebugLineStrRef",
        AttributeValue::DwoId(_) => "DwoId",
    }
}
