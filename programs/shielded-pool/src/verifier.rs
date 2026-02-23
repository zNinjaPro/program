use anchor_lang::prelude::*;
use crate::errors::ShieldedPoolError;
use crate::state::VerifierConfig;

// SAFETY: mock-verifier accepts all-zero proofs. It must NEVER be enabled
// alongside altbn128_syscalls (the real verifier used on devnet/mainnet).
// If both features are active, the build will fail at compile time.
#[cfg(all(feature = "mock-verifier", feature = "altbn128_syscalls"))]
compile_error!(
    "SECURITY: `mock-verifier` and `altbn128_syscalls` cannot be enabled together. \
     `mock-verifier` accepts all-zero proofs and must never be used in production builds."
);

/// BN254 field prime (alt_bn128)
/// p = 21888242871839275222246405745257275088548364400416034343698204186575808495617
const BN254_FIELD_SIZE: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29,
    0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x97, 0x81, 0x6a, 0x91, 0x68, 0x71, 0xca, 0x8d,
    0x3c, 0x20, 0x8c, 0x16, 0xd8, 0x7c, 0xfd, 0x47,
];

/// Groth16 proof structure for BN254
/// Proof consists of 3 G1 points (pi_a, pi_c) and 1 G2 point (pi_b)
/// G1 point: 64 bytes (x, y coordinates, each 32 bytes)
/// G2 point: 128 bytes (x = (x1, x2), y = (y1, y2), each component 32 bytes)
#[derive(Clone, Debug)]
pub struct Groth16Proof {
    pub pi_a: [u8; 64],      // G1 point
    pub pi_b: [u8; 128],     // G2 point
    pub pi_c: [u8; 64],      // G1 point
}

impl Groth16Proof {
    pub const SIZE: usize = 256; // 64 + 128 + 64

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        require!(bytes.len() >= Self::SIZE, ShieldedPoolError::InvalidProof);

        let mut pi_a = [0u8; 64];
        let mut pi_b = [0u8; 128];
        let mut pi_c = [0u8; 64];

        pi_a.copy_from_slice(&bytes[0..64]);
        pi_b.copy_from_slice(&bytes[64..192]);
        pi_c.copy_from_slice(&bytes[192..256]);

        Ok(Self { pi_a, pi_b, pi_c })
    }
}

/// Groth16 verifying key for BN254
/// Minimal representation - in production, this would be stored in a PDA
#[derive(Clone, Debug)]
pub struct VerifyingKey {
    pub alpha_g1: [u8; 64],
    pub beta_g2: [u8; 128],
    pub gamma_g2: [u8; 128],
    pub delta_g2: [u8; 128],
    pub ic: Vec<[u8; 64]>, // IC points for public inputs
}

impl VerifyingKey {
    /// Create a dummy/test verifying key
    /// In production, this would be loaded from a VerifierConfig PDA
    pub fn test_key(num_public_inputs: usize) -> Self {
        Self {
            alpha_g1: [0u8; 64],
            beta_g2: [0u8; 128],
            gamma_g2: [0u8; 128],
            delta_g2: [0u8; 128],
            ic: vec![[0u8; 64]; num_public_inputs + 1], // +1 for the constant term
        }
    }
}

impl From<&VerifierConfig> for VerifyingKey {
    fn from(config: &VerifierConfig) -> Self {
        let alpha = pack_g1(&config.vk_galpha);
        let beta = pack_g2(&config.vk_hbeta);
        let gamma = pack_g2(&config.vk_ggamma);
        let delta = pack_g2(&config.vk_hdelta);

        let ic_len = config.vk_len_ic as usize;
        let mut ic = Vec::with_capacity(ic_len);
        for point in config.ic.iter().take(ic_len) {
            ic.push(*point);
        }

        Self {
            alpha_g1: alpha,
            beta_g2: beta,
            gamma_g2: gamma,
            delta_g2: delta,
            ic,
        }
    }
}

/// Verify a Groth16 proof using BN254 curve operations
/// 
/// The Groth16 verification equation is:
/// e(pi_a, pi_b) = e(alpha, beta) * e(vk_x, gamma) * e(pi_c, delta)
/// where vk_x = IC[0] + sum(IC[i+1] * public_input[i])
///
/// This implementation uses Solana's alt_bn128 syscalls for efficient verification.
/// For full production deployment, consider using a specialized library like groth16-solana.
pub fn verify_groth16_proof(
    proof: &Groth16Proof,
    public_inputs: &[[u8; 32]],
    vk: &VerifyingKey,
) -> Result<bool> {
    // Validate public inputs count
    require!(
        public_inputs.len() + 1 == vk.ic.len(),
        ShieldedPoolError::InvalidPublicInputs
    );

    // Basic validation: Check proof points are not zero
    // Exception allowed only under mock-verifier feature for local/dev testing
    #[cfg(feature = "mock-verifier")]
    if proof.pi_a.iter().all(|&b| b == 0)
        && proof.pi_b.iter().all(|&b| b == 0)
        && proof.pi_c.iter().all(|&b| b == 0)
    {
        return Ok(true);
    }

    if proof.pi_a.iter().all(|&b| b == 0) {
        return Ok(false);
    }
    
    if proof.pi_c.iter().all(|&b| b == 0) {
        return Ok(false);
    }
    
    // Validate public inputs are in field (< BN254_FIELD_SIZE)
    for input in public_inputs {
        if !is_in_field(input) {
            return Err(ShieldedPoolError::InvalidPublicInputs.into());
        }
    }

    // Compute vk_x = IC[0] + sum(IC[i+1] * public_input[i])
    // This requires elliptic curve operations on BN254 G1
    let vk_x = compute_vk_x(&vk.ic, public_inputs)?;

    // Verify the pairing equation using alt_bn128 operations
    // e(pi_a, pi_b) == e(alpha, beta) * e(vk_x, gamma) * e(pi_c, delta)
    // 
    // This can be rewritten as:
    // e(-pi_a, pi_b) * e(alpha, beta) * e(vk_x, gamma) * e(pi_c, delta) == 1
    let result = verify_pairing_equation(
        proof,
        &vk_x,
        &vk.alpha_g1,
        &vk.beta_g2,
        &vk.gamma_g2,
        &vk.delta_g2,
    )?;

    Ok(result)
}

/// Compute vk_x = IC[0] + sum(IC[i+1] * public_input[i])
/// This requires scalar multiplication and point addition on BN254 G1
fn compute_vk_x(ic: &[[u8; 64]], public_inputs: &[[u8; 32]]) -> Result<[u8; 64]> {
    require!(ic.len() == public_inputs.len() + 1, ShieldedPoolError::InvalidPublicInputs);

    // Start with IC[0] (the constant term)
    let mut vk_x = ic[0];

    // Add sum(IC[i+1] * public_input[i])
    for (i, input) in public_inputs.iter().enumerate() {
        // Scalar multiply: IC[i+1] * input
        // Format input as [x(32), y(32), scalar(32)] = 96 bytes
        let mut mul_input = [0u8; 96];
        mul_input[..64].copy_from_slice(&ic[i + 1]);
        mul_input[64..].copy_from_slice(input);
        
        let point = crate::syscalls::alt_bn128_g1_mul(&mul_input)?;
        
        // Add to accumulator: vk_x += point
        // Format input as [x1(32), y1(32), x2(32), y2(32)] = 128 bytes
        let mut add_input = [0u8; 128];
        add_input[..64].copy_from_slice(&vk_x);
        add_input[64..].copy_from_slice(&point);
        
        vk_x = crate::syscalls::alt_bn128_g1_add(&add_input)?;
    }

    Ok(vk_x)
}

/// Verify the pairing equation for Groth16
/// Checks: e(-pi_a, pi_b) * e(alpha, beta) * e(vk_x, gamma) * e(pi_c, delta) == 1
fn verify_pairing_equation(
    proof: &Groth16Proof,
    vk_x: &[u8; 64],
    alpha_g1: &[u8; 64],
    beta_g2: &[u8; 128],
    gamma_g2: &[u8; 128],
    delta_g2: &[u8; 128],
) -> Result<bool> {
    // Negate pi_a to get -pi_a (for the pairing check)
    let neg_pi_a = negate_g1(&proof.pi_a)?;
    
    // Encode pairing input: 4 pairings, each with G1 point (64 bytes) + G2 point (128 bytes)
    // Format: [G1_1, G2_1, G1_2, G2_2, G1_3, G2_3, G1_4, G2_4]
    // Total: 4 * (64 + 128) = 768 bytes
    let mut pairing_input = Vec::with_capacity(768);
    
    // Pairing 1: e(-pi_a, pi_b)
    pairing_input.extend_from_slice(&neg_pi_a);
    pairing_input.extend_from_slice(&proof.pi_b);
    
    // Pairing 2: e(alpha, beta)
    pairing_input.extend_from_slice(alpha_g1);
    pairing_input.extend_from_slice(beta_g2);
    
    // Pairing 3: e(vk_x, gamma)
    pairing_input.extend_from_slice(vk_x);
    pairing_input.extend_from_slice(gamma_g2);
    
    // Pairing 4: e(pi_c, delta)
    pairing_input.extend_from_slice(&proof.pi_c);
    pairing_input.extend_from_slice(delta_g2);
    
    // Call alt_bn128_pairing syscall to verify the equation
    let is_valid = crate::syscalls::verify_alt_bn128_pairing(&pairing_input)?;
    
    Ok(is_valid)
}

/// Negate a G1 point (x, y) -> (x, -y mod p)
fn negate_g1(point: &[u8; 64]) -> Result<[u8; 64]> {
    let mut negated = [0u8; 64];

    // Copy x coordinate unchanged
    negated[..32].copy_from_slice(&point[..32]);

    // Extract y coordinate as an owned array for validation
    let mut y_coord = [0u8; 32];
    y_coord.copy_from_slice(&point[32..64]);

    // Check if point is at infinity (y = 0)
    if y_coord.iter().all(|&b| b == 0) {
        negated[32..].copy_from_slice(&y_coord);
        return Ok(negated);
    }

    // Reject points with non-field y-coordinates to avoid modular underflow
    if !is_in_field(&y_coord) {
        return Err(ShieldedPoolError::InvalidProof.into());
    }

    let negated_y = field_negate(&y_coord)?;
    negated[32..].copy_from_slice(&negated_y);

    Ok(negated)
}

/// Compute -(value) mod p for the BN254 field.
/// Returns error if value is not a field element (< p) to avoid subtract underflow.
fn field_negate(value: &[u8; 32]) -> Result<[u8; 32]> {
    if value.iter().all(|&b| b == 0) {
        return Ok([0u8; 32]);
    }

    let mut result = [0u8; 32];
    let mut borrow: i32 = 0;

    for i in (0..32).rev() {
        let pi = BN254_FIELD_SIZE[i] as i32;
        let yi = value[i] as i32;
        let mut limb = pi - yi - borrow;
        if limb < 0 {
            limb += 256;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result[i] = limb as u8;
    }

    if borrow != 0 {
        return Err(ShieldedPoolError::InvalidProof.into());
    }

    Ok(result)
}

/// Check if a value is in the BN254 scalar field
fn is_in_field(value: &[u8; 32]) -> bool {
    // Compare value < BN254_FIELD_SIZE (big-endian)
    for i in 0..32 {
        if value[i] < BN254_FIELD_SIZE[i] {
            return true;
        } else if value[i] > BN254_FIELD_SIZE[i] {
            return false;
        }
    }
    false // value == BN254_FIELD_SIZE, which is not in field
}

fn pack_g1(coords: &[[u8; 32]; 2]) -> [u8; 64] {
    let mut packed = [0u8; 64];
    packed[..32].copy_from_slice(&coords[0]);
    packed[32..].copy_from_slice(&coords[1]);
    packed
}

fn pack_g2(coords: &[[u8; 32]; 4]) -> [u8; 128] {
    let mut packed = [0u8; 128];
    packed[..32].copy_from_slice(&coords[0]);
    packed[32..64].copy_from_slice(&coords[1]);
    packed[64..96].copy_from_slice(&coords[2]);
    packed[96..].copy_from_slice(&coords[3]);
    packed
}

/// Decode public inputs from proof bytes
/// Public inputs are packed as 32-byte big-endian field elements
pub fn decode_public_inputs(
    public_input_bytes: &[[u8; 32]],
) -> Result<Vec<[u8; 32]>> {
    // Validate each input is in field
    for input in public_input_bytes {
        require!(
            is_in_field(input),
            ShieldedPoolError::InvalidPublicInputs
        );
    }
    
    Ok(public_input_bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_from_bytes() {
        let proof_bytes = vec![1u8; Groth16Proof::SIZE];
        let proof = Groth16Proof::from_bytes(&proof_bytes);
        assert!(proof.is_ok());

        let proof = proof.unwrap();
        assert_eq!(proof.pi_a[0], 1);
        assert_eq!(proof.pi_b[0], 1);
        assert_eq!(proof.pi_c[0], 1);
    }

    #[test]
    fn test_proof_invalid_size() {
        let proof_bytes = vec![1u8; 100]; // Too short
        let proof = Groth16Proof::from_bytes(&proof_bytes);
        assert!(proof.is_err());
    }

    #[test]
    fn test_is_in_field() {
        // Zero is in field
        let zero = [0u8; 32];
        assert!(is_in_field(&zero));

        // Small value is in field
        let mut small = [0u8; 32];
        small[31] = 42;
        assert!(is_in_field(&small));

        // Field size itself is NOT in field
        assert!(!is_in_field(&BN254_FIELD_SIZE));

        // Value > field size is not in field
        let mut too_large = BN254_FIELD_SIZE;
        too_large[31] = too_large[31].wrapping_add(1);
        assert!(!is_in_field(&too_large));
    }

    #[test]
    fn test_verify_rejects_zero_proof() {
        let mut proof = Groth16Proof {
            pi_a: [0u8; 64],
            pi_b: [1u8; 128],
            pi_c: [1u8; 64],
        };
        let public_inputs = vec![[0u8; 32]; 3];
        let vk = VerifyingKey::test_key(3);

        // Zero pi_a should fail
        let result = verify_groth16_proof(&proof, &public_inputs, &vk);
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Non-zero pi_a but zero pi_c should fail
        proof.pi_a = [1u8; 64];
        proof.pi_c = [0u8; 64];
        let result = verify_groth16_proof(&proof, &public_inputs, &vk);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_verify_accepts_valid_structure() {
        let proof = Groth16Proof {
            pi_a: [1u8; 64],
            pi_b: [1u8; 128],
            pi_c: [1u8; 64],
        };
        let public_inputs = vec![[0u8; 32]; 3];
        let vk = VerifyingKey::test_key(3);

        let result = verify_groth16_proof(&proof, &public_inputs, &vk);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Placeholder returns true for non-zero
    }

    #[test]
    fn test_verify_wrong_input_count() {
        let proof = Groth16Proof {
            pi_a: [1u8; 64],
            pi_b: [1u8; 128],
            pi_c: [1u8; 64],
        };
        let public_inputs = vec![[0u8; 32]; 5]; // Wrong count
        let vk = VerifyingKey::test_key(3);

        let result = verify_groth16_proof(&proof, &public_inputs, &vk);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_out_of_field_input() {
        let proof = Groth16Proof {
            pi_a: [1u8; 64],
            pi_b: [1u8; 128],
            pi_c: [1u8; 64],
        };
        
        // Create input larger than field size
        let mut large_input = BN254_FIELD_SIZE;
        large_input[31] = large_input[31].wrapping_add(1);
        let public_inputs = vec![large_input];
        let vk = VerifyingKey::test_key(1);

        let result = verify_groth16_proof(&proof, &public_inputs, &vk);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_public_inputs() {
        let inputs = vec![[0u8; 32], [1u8; 32], [2u8; 32]];
        let decoded = decode_public_inputs(&inputs);
        assert!(decoded.is_ok());
        assert_eq!(decoded.unwrap().len(), 3);
    }

    #[test]
    fn test_decode_rejects_out_of_field() {
        let mut large = BN254_FIELD_SIZE;
        large[31] = large[31].wrapping_add(1);
        let inputs = vec![large];
        let decoded = decode_public_inputs(&inputs);
        assert!(decoded.is_err());
    }

    #[test]
    fn test_negate_g1_negates_one() {
        let mut point = [0u8; 64];
        point[63] = 1; // y = 1

        let negated = negate_g1(&point).expect("negation should succeed");

        let mut expected_y = BN254_FIELD_SIZE;
        expected_y[31] = expected_y[31].wrapping_sub(1);

        assert_eq!(&negated[32..], &expected_y);
    }

    #[test]
    fn test_negate_g1_rejects_out_of_field() {
        let mut point = [0u8; 64];
        point[32..64].copy_from_slice(&BN254_FIELD_SIZE); // y == p (invalid field element)

        let err = negate_g1(&point);
        assert!(err.is_err());
    }
}
