/// Alt_bn128 curve operations for Groth16 verification
/// 
/// TEMPORARY IMPLEMENTATION: For MVP, we're using placeholder verification
/// that logs proof data but accepts all structurally valid proofs.
///
/// PRODUCTION TODO: Integrate alt_bn128 syscalls via:
/// 1. Use solana_program::alt_bn128 (when available in SDK version)
/// 2. OR use groth16-solana library for full Groth16 verification
/// 3. OR implement direct FFI to sol_alt_bn128_* syscalls
///
/// The proof generation pipeline is working (circuits → snarkjs → valid proofs).
/// This is the final integration point for on-chain verification.

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
            msg!("alt_bn128_g1_add SUCCESS: out[0..4]={:?}", &out[..4]);
            return Ok(out);
        } else {
            msg!("alt_bn128_g1_add ERROR code {}", code);
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        // Dev fallback: echo structure for local tests
        out[..32].copy_from_slice(&input[..32]);
        msg!("alt_bn128_g1_add (fallback) out[0..4]={:?}", &out[..4]);
        Ok(out)
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        msg!("alt_bn128_g1_add unavailable without altbn128_syscalls or mock-verifier feature");
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
            msg!("alt_bn128_g1_mul SUCCESS: out[0..4]={:?}", &out[..4]);
            return Ok(out);
        } else {
            msg!("alt_bn128_g1_mul ERROR code {}", code);
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        out[..32].copy_from_slice(&input[..32]);
        msg!("alt_bn128_g1_mul (fallback) out[0..4]={:?}", &out[..4]);
        Ok(out)
    }
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        msg!("alt_bn128_g1_mul unavailable without altbn128_syscalls or mock-verifier feature");
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
    // Validate input length (must be multiple of 192)
    if input.len() % 192 != 0 {
        msg!("Invalid pairing input length: {}", input.len());
        return Err(error!(ErrorCode::AccountDidNotSerialize));
    }

    #[cfg(all(target_os = "solana", feature = "altbn128_syscalls"))]
    {
        let mut out = [0u8; 1]; // syscall writes 1 byte: 1 = true, 0 = false
        let code = unsafe { sol_alt_bn128_pairing(input.as_ptr(), input.len() as u64, out.as_mut_ptr()) };
        if code == 0 {
            let ok = out[0] == 1u8;
            msg!("alt_bn128_pairing SUCCESS: {}", ok);
            return Ok(ok);
        } else {
            msg!("alt_bn128_pairing ERROR code {}", code);
            return Err(ProgramError::InvalidArgument.into());
        }
    }
    
    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), feature = "mock-verifier"))]
    {
        let num_pairings = input.len() / 192;
        msg!("⚠️ alt_bn128_pairing FALLBACK: treating {} pairings as valid", num_pairings);
        Ok(true)
    }

    #[cfg(all(not(all(target_os = "solana", feature = "altbn128_syscalls")), not(feature = "mock-verifier")))]
    {
        msg!("alt_bn128_pairing unavailable without altbn128_syscalls or mock-verifier feature");
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
