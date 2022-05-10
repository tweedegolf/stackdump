use super::{TypeValue, AddressType};

impl<ADDR: AddressType> TypeValue<ADDR> {
    pub fn render_value(&self) -> String {
        "VALUE".into()
    }
}
