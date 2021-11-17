use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CortexMFpuRegisters {
    registers: [u32; 32],
    fpscr: u32,
}

impl Default for CortexMFpuRegisters {
    fn default() -> Self {
        Self {
            registers: [0; 32],
            fpscr: 0,
        }
    }
}

impl CortexMFpuRegisters {
    #[cfg(feature = "capture")]
    #[inline(always)]
    pub(crate) fn capture(&mut self) {
        unsafe {
            asm!(
                "vstr s0, [{0}, #0]",
                "vstr s1, [{0}, #4]",
                "vstr s2, [{0}, #8]",
                "vstr s3, [{0}, #12]",
                "vstr s4, [{0}, #16]",
                "vstr s5, [{0}, #20]",
                "vstr s6, [{0}, #24]",
                "vstr s7, [{0}, #28]",
                "vstr s8, [{0}, #32]",
                "vstr s9, [{0}, #36]",
                "vstr s10, [{0}, #40]",
                "vstr s11, [{0}, #44]",
                "vstr s12, [{0}, #48]",
                "vstr s13, [{0}, #52]",
                "vstr s14, [{0}, #56]",
                "vstr s15, [{0}, #60]",
                "vstr s16, [{0}, #64]",
                "vstr s17, [{0}, #68]",
                "vstr s18, [{0}, #72]",
                "vstr s19, [{0}, #76]",
                "vstr s20, [{0}, #80]",
                "vstr s21, [{0}, #84]",
                "vstr s22, [{0}, #88]",
                "vstr s23, [{0}, #92]",
                "vstr s24, [{0}, #96]",
                "vstr s25, [{0}, #100]",
                "vstr s26, [{0}, #104]",
                "vstr s27, [{0}, #108]",
                "vstr s28, [{0}, #112]",
                "vstr s29, [{0}, #116]",
                "vstr s30, [{0}, #120]",
                "vstr s31, [{0}, #124]",
                "vmrs {tmp}, fpscr",
                "str {tmp}, [{1}]",
                in(reg) self.registers.as_ptr(),
                in(reg) &mut self.fpscr as *mut u32,
                tmp = out(reg) _,
            );
        }
    }

    pub fn fpu_register(&self, index: usize) -> &u32 {
        &self.registers[index]
    }

    pub fn fpu_register_mut(&mut self, index: usize) -> &mut u32 {
        &mut self.registers[index]
    }

    pub fn fpscr(&self) -> &u32 {
        &self.fpscr
    }

    pub fn fpscr_mut(&mut self) -> &mut u32 {
        &mut self.fpscr
    }
}
