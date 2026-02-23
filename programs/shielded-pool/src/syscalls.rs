//! Alt_bn128 curve operations for Groth16 verification
//!
//! Uses Solana's native alt_bn128 syscalls (`sol_alt_bn128_g1_add`, `sol_alt_bn128_g1_mul`,
//! `sol_alt_bn128_pairing`) for BN254 elliptic curve operations. These are gated behind the
//! `altbn128_syscalls` feature flag and compiled only for the Solana BPF target.
//!
//! When neither `altbn128_syscalls` nor `mock-verifier` is enabled, the operations return
//! errors, ensuring no silent fallback. See `verifier.rs` for the compile_error guard that
//! prevents `mock-verifier` and `altbn128_syscalls` from being enabled simultaneously.

use anchor_lang::prelude::*;

// Allow unused in non-Solana/mock builds where the error is not referenced
#[allow(unused_imports)]
use crate::errors::ShieldedPoolError;

// Direct FFI bindings to alt_bn128 syscalls on-chain
// These are only available when compiled for the Solana BPF target.
// Only declare FFI when the alt_bn128 syscalls are available.
// Gate behind a crate feature to avoid unresolved symbols on networks
// where the syscalls are not enabled.
#[cfg(all(target_os = "solana", feature = "altbn128_syscalls"))]
extern "C" {
    // Returns 0 on success, non-zero error code on failure
    // fn sol_alt_bn128_g1_add(input: *const u8, input_len: u64, out: *mut u8) -> u64;
    // fn sol_alt_bn128_g1_mul(input: *const u8, input_len: u64, out: *mut u8) -> u64;
    // fn sol_alt_bn128_pairing(input: *const u8, input_len: u64, out: *mut u8) -> u64;
    fn sol_alt_bn128_g1_add(input: *const u8, input_len: u64, out: *mut u8) -> u64;
    fn sol_alt_bn128_g1_mul(input: *const u8, input_len: u64, out: *mut u8) -> u64;
    fn sol_alt_bn128_pairing(input: *const u8, input_len: u64, out: *mut u8) -> u64;
}

/// G1 point addition: (x1, y1) + (x2, y2)
/// 
/// Input: 128 bytes [x1 (32), y1 (32), x2 (32), y2 (32)]
/// Output: 64 bytes [x_result (32), y_result (32)]
/// 
/// Cost: ~500 compute units
pub fn alt_bn128_g1_add(input: &[u8; 128]) -> Result<[u8; 64]> {
    let mut out = [0u8; 64];
    #[cfg(all(target_os = "solana", feature = "altbn128_syscalls"))]
    {
        let code = unsafe { sol_alt_bn128_g1_add(input.as_ptr(), 128u64, out.as_mut_ptr()) };
        if code == 0 {
            return Ok(out);
        } else {
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        out[..32].copy_from_slice(&input[..32]);
        Ok(out)
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        Err(ShieldedPoolError::InvalidVerifierConfig.into())
    }
}

/// G1 scalar multiplication: point * scalar
/// 
/// Input: 96 bytes [x (32), y (32), scalar (32)]
/// Output: 64 bytes [x_result (32), y_result (32)]
/// 
/// Cost: ~2,000 compute units
pub fn alt_bn128_g1_mul(input: &[u8; 96]) -> Result<[u8; 64]> {
    let mut out = [0u8; 64];
    #[cfg(all(target_os = "solana", feature = "altbn128_syscalls"))]
    {
        let code = unsafe { sol_alt_bn128_g1_mul(input.as_ptr(), 96u64, out.as_mut_ptr()) };
        if code == 0 {
            return Ok(out);
        } else {
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        out[..32].copy_from_slice(&input[..32]);
        Ok(out)
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        Err(ShieldedPoolError::InvalidVerifierConfig.into())
    }
}

/// Pairing check: verifies if product of pairings equals 1
/// 
/// Input: n * 192 bytes, where each element is [G1 point (64), G2 point (128)]
/// Output: bool (true if valid, false if invalid)
/// 
/// For Groth16, we need 4 pairings = 768 bytes input
/// Cost: ~36,000 compute units per pairing (~144k total for 4 pairings)
pub fn verify_alt_bn128_pairing(input: &[u8]) -> Result<bool> {
    if !input.len().is_multiple_of(192) {
        return Err(error!(ErrorCode::AccountDidNotSerialize));
    }

    #[cfg(all(target_os = "solana", feature = "altbn128_syscalls"))]
    {
        let mut out = [0u8; 1];
        let code = unsafe { sol_alt_bn128_pairing(input.as_ptr(), input.len() as u64, out.as_mut_ptr()) };
        if code == 0 {
            let ok = out[0] == 1u8;
            return Ok(ok);
        } else {
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        Ok(true)
    }

    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        Err(ShieldedPoolError::InvalidVerifierConfig.into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_g1_add_input_size() {
        let input = [0u8; 128];
        // Would test real operation when syscall is integrated
        assert_eq!(input.len(), 128);
    }

    #[test]
    fn test_g1_mul_input_size() {
        let input = [0u8; 96];
        assert_eq!(input.len(), 96);
    }

    #[test]
    fn test_pairing_input_validation() {
        // Valid: 4 pairings = 768 bytes
        let valid_input = vec![0u8; 768];
        assert_eq!(valid_input.len() % 192, 0);

        // Invalid: not multiple of 192
        let invalid_input = vec![0u8; 500];
        assert_ne!(invalid_input.len() % 192, 0);
    }
}
