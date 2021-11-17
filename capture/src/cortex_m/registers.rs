use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct CortexMBaseRegisters([u32; 17]);

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
    #[cfg(feature = "capture")]
    #[inline(always)]
    pub(crate) fn capture(&mut self) {
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
                "mrs {tmp}, apsr", // We can't get the program status register normally, so store it in tmp
                "str {tmp}, [{0}, #64]",
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

    pub fn psr(&self) -> &u32 {
        &self.0[16]
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

    pub fn psr_mut(&mut self) -> &mut u32 {
        &mut self.0[16]
    }
}

impl Default for CortexMBaseRegisters {
    fn default() -> Self {
        Self([0; 17])
    }
}
