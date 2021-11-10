use stackdump_core::Registers;

#[derive(Clone)]
pub struct CortexMRegisters {
    values: [u32; 16],
    #[cfg(feature = "cortex-m-fpu")]
    fpu_registers: fpu::FpuRegisters,
}

impl core::fmt::Debug for CortexMRegisters {
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
            .field("fpu_registers", &self.fpu_registers)
            .finish()
    }
}

impl CortexMRegisters {
    pub fn register(&self, index: usize) -> &u32 {
        &self.values[index]
    }

    pub fn sp(&self) -> &u32 {
        &self.values[13]
    }

    pub fn lr(&self) -> &u32 {
        &self.values[14]
    }

    pub fn pc(&self) -> &u32 {
        &self.values[15]
    }

    pub fn register_mut(&mut self, index: usize) -> &mut u32 {
        &mut self.values[index]
    }

    pub fn sp_mut(&mut self) -> &mut u32 {
        &mut self.values[13]
    }

    pub fn lr_mut(&mut self) -> &mut u32 {
        &mut self.values[14]
    }

    pub fn pc_mut(&mut self) -> &mut u32 {
        &mut self.values[15]
    }
}

impl Default for CortexMRegisters {
    fn default() -> Self {
        Self {
            values: [0; 16],
            #[cfg(feature = "cortex-m-fpu")]
            fpu_registers: Default::default(),
        }
    }
}

impl stackdump_core::Registers for CortexMRegisters {
    fn capture(&mut self) {
        unsafe {
            let mut pc = 0u32;
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
                "mov {1}, pc",
                in(reg) self.values.as_ptr(),
                out(reg) pc,
            );
            *self.pc_mut() = pc;
        }

        #[cfg(feature = "cortex-m-fpu")]
        self.fpu_registers.capture();
    }
}

#[cfg(feature = "cortex-m-fpu")]
mod fpu {
    #[derive(Clone, Debug)]
    pub struct FpuRegisters {
        values: [u32; 32],
    }

    impl Default for FpuRegisters {
        fn default() -> Self {
            Self { values: [0; 32] }
        }
    }

    impl stackdump_core::Registers for FpuRegisters {
        fn capture(&mut self) {
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
                    in(reg) self.values.as_ptr(),
                );
            }
        }
    }
}
