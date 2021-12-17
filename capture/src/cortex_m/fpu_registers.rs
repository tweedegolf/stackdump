use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CortexMFpuRegisters([u32; 32]);

impl Default for CortexMFpuRegisters {
    fn default() -> Self {
        Self([0; 32])
    }
}

impl CortexMFpuRegisters {
    #[cfg(feature = "capture")]
    #[inline(always)]
    pub(crate) fn capture(&mut self) {
        use core::arch::asm;

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
                in(reg) self.0.as_ptr(),
            );
        }
    }

    pub fn fpu_register(&self, index: usize) -> &u32 {
        &self.0[index]
    }

    pub fn fpu_register_mut(&mut self, index: usize) -> &mut u32 {
        &mut self.0[index]
    }

    pub fn copy_bytes(&self) -> [u8; 32 * 4] {
        let mut bytes = [0; 32 * 4];
        for (i, r) in self.0.iter().enumerate() {
            bytes[i * 4..][..4].copy_from_slice(&r.to_le_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: [u8; 32 * 4]) -> Self {
        let mut s = Self::default();

        for (i, r) in bytes.chunks(4).enumerate() {
            s.0[i] = u32::from_le_bytes(r.try_into().unwrap());
        }

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes() {
        let mut registers = CortexMFpuRegisters::default();
        // Set the registers to a random value. This also sets the highest bytes for some registers, so endianness is (kind of) tested
        for i in 0..32 {
            *registers.fpu_register_mut(i as usize) = 1 << i;
        }

        let bytes = registers.copy_bytes();
        let copy_registers = CortexMFpuRegisters::from_bytes(bytes);

        assert_eq!(registers, copy_registers);
    }
}
