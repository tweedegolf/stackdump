use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct CortexMBaseRegisters([u32; 16]);

impl core::fmt::Debug for CortexMBaseRegisters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CortexMRegisters")
            .field("r0", self.register(0))
            .field("r1", self.register(1))
            .field("r2", self.register(2))
            .field("r3", self.register(3))
            .field("r4", self.register(4))
            .field("r5", self.register(5))
            .field("r6", self.register(6))
            .field("r7", self.register(7))
            .field("r8", self.register(8))
            .field("r9", self.register(9))
            .field("r10", self.register(10))
            .field("r11", self.register(11))
            .field("r12", self.register(12))
            .field("sp", self.sp())
            .field("lr", self.lr())
            .field("pc", self.pc())
            .finish()
    }
}

impl CortexMBaseRegisters {
    #[cfg(all(feature = "capture", cortex_m))]
    #[inline(always)]
    pub(crate) fn capture(&mut self) {
        use core::arch::asm;

        unsafe {
            asm!(
                "str r0, [{0}, #0]",
                "str r1, [{0}, #4]",
                "str r2, [{0}, #8]",
                "str r3, [{0}, #12]",
                "str r4, [{0}, #16]",
                "str r5, [{0}, #20]",
                "str r6, [{0}, #24]",
                "str r7, [{0}, #28]",
                "str r8, [{0}, #32]",
                "str r9, [{0}, #36]",
                "str r10, [{0}, #40]",
                "str r11, [{0}, #44]",
                "str r12, [{0}, #48]",
                "str sp, [{0}, #52]",
                "str lr, [{0}, #56]",
                "mov {tmp}, pc", // We can't use the str instruction with the PC register directly, so store it in tmp
                "str {tmp}, [{0}, #60]",
                in(reg) self.0.as_ptr(),
                tmp = out(reg) _,
            );
        }
    }

    pub fn register(&self, index: usize) -> &u32 {
        &self.0[index]
    }

    pub fn sp(&self) -> &u32 {
        &self.0[13]
    }

    pub fn lr(&self) -> &u32 {
        &self.0[14]
    }

    pub fn pc(&self) -> &u32 {
        &self.0[15]
    }

    pub fn register_mut(&mut self, index: usize) -> &mut u32 {
        &mut self.0[index]
    }

    pub fn sp_mut(&mut self) -> &mut u32 {
        &mut self.0[13]
    }

    pub fn lr_mut(&mut self) -> &mut u32 {
        &mut self.0[14]
    }

    pub fn pc_mut(&mut self) -> &mut u32 {
        &mut self.0[15]
    }

    pub fn copy_bytes(&self) -> [u8; 16 * 4] {
        let mut bytes = [0; 16 * 4];
        for (i, r) in self.0.iter().enumerate() {
            bytes[i * 4..][..4].copy_from_slice(&r.to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: [u8; 16 * 4]) -> Self {
        let mut s = Self::default();

        for (i, r) in bytes.chunks(4).enumerate() {
            s.0[i] = u32::from_le_bytes(r.try_into().unwrap());
        }

        s
    }
}

impl Default for CortexMBaseRegisters {
    fn default() -> Self {
        Self([0; 16])
    }
}
