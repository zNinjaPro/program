use anchor_lang::prelude::*;
use core::cmp::Ordering;

/// BN254 field modulus (big-endian)
pub const BN254_MODULUS: [u8; 32] =
    hex_literal::hex!("30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001");

fn be_cmp(a: &[u8; 32], b: &[u8; 32]) -> Ordering {
    for i in 0..32 {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    Ordering::Equal
}

fn be_sub(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut borrow: u16 = 0;
    for i in (0..32).rev() {
        let ai = a[i] as u16;
        let bi = b[i] as u16 + borrow;
        if ai >= bi {
            out[i] = (ai - bi) as u8;
            borrow = 0;
        } else {
            out[i] = ((ai + 256) - bi) as u8;
            borrow = 1;
        }
    }
    out
}

/// Reduces a 32-byte big-endian value into the BN254 scalar field.
/// For 256-bit inputs, this requires at most ~4 subtractions of the
/// BN254 modulus (~2^254), which is acceptable for on-chain compute.
pub fn reduce_bytes_to_field(bytes: &[u8; 32]) -> [u8; 32] {
    let mut value = *bytes;
    while be_cmp(&value, &BN254_MODULUS) != Ordering::Less {
        value = be_sub(&value, &BN254_MODULUS);
    }
    value
}

/// Convert a `Pubkey` into a BN254 field element deterministically.
pub fn pubkey_to_field(key: &Pubkey) -> [u8; 32] {
    let bytes = key.to_bytes();
    reduce_bytes_to_field(&bytes)
}
