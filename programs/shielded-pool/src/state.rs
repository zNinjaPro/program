use anchor_lang::prelude::*;

pub const MERKLE_DEPTH: usize = 32;
pub const ROOT_HISTORY: usize = 64;
pub const MAX_NULLIFIER_CHUNK_SIZE: usize = 256;
pub const LEAF_CHUNK_SIZE: usize = 256;
pub const MAX_VERIFIER_IC_POINTS: usize = 64;

// Chain ID for Solana networks
// For devnet/testnet: use a test value
// For mainnet-beta: should be updated to a production value
pub const CHAIN_ID: [u8; 32] = [0u8; 32];

#[account]
pub struct PoolConfig {
    pub version: u8,
    pub mint: Pubkey,
    pub vault_authority: Pubkey,
    pub merkle_depth: u8,
    pub root_history: u16,
    pub program_bump: u8,
    pub tree_bump: u8,
    pub nullifier_chunk_size: u16,
    pub reserved: [u8; 16],
}

impl PoolConfig {
    pub const LEN: usize = 8 + 1 + 32 + 32 + 1 + 2 + 1 + 1 + 2 + 16;
}

#[account(zero_copy)]
#[repr(C)]
pub struct PoolTree {
    pub depth: u8,
    pub _padding0: [u8; 7],
    pub next_index: u64,
    pub frontier: [[u8; 32]; MERKLE_DEPTH],
    pub roots: [[u8; 32]; ROOT_HISTORY],
    pub roots_len: u8,
    pub roots_head: u8,
    pub zero_hashes: [[u8; 32]; MERKLE_DEPTH],
    pub _padding_end: [u8; 6],
}

impl PoolTree {
    // Size of data excluding the 8-byte discriminator
    pub const LEN: usize = 1 + 7 + 8 + (32 * MERKLE_DEPTH) + (32 * ROOT_HISTORY) + 1 + 1 + (32 * MERKLE_DEPTH) + 6;
}

#[account(zero_copy)]
#[repr(C)]
pub struct NullifierChunk {
    pub pool_id: [u8; 32],
    pub chunk_index: u32,
    pub count: u16,
    pub _padding: [u8; 2], // Explicit padding for alignment
    pub nodes: [[u8; 32]; MAX_NULLIFIER_CHUNK_SIZE],
}

impl NullifierChunk {
    // Size of data excluding the 8-byte discriminator
    pub const LEN: usize = 32 + 4 + 2 + 2 + (32 * MAX_NULLIFIER_CHUNK_SIZE);
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CircuitType {
    Withdraw = 0,
    ShieldedTransfer = 1,
}

impl CircuitType {
    pub const fn seed(self) -> &'static [u8; 8] {
        match self {
            CircuitType::Withdraw => b"withdraw",
            CircuitType::ShieldedTransfer => b"transfer",
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CircuitType::Withdraw),
            1 => Some(CircuitType::ShieldedTransfer),
            _ => None,
        }
    }
}

#[account(zero_copy)]
#[repr(C)]
pub struct VerifierConfig {
    pub version: u16,
    pub circuit: u8,
    pub _padding0: u8,
    pub vk_len_ic: u16,
    pub _padding1: u16,
    pub vk_galpha: [[u8; 32]; 2],
    pub vk_hbeta: [[u8; 32]; 4],
    pub vk_ggamma: [[u8; 32]; 4],
    pub vk_hdelta: [[u8; 32]; 4],
    pub ic: [[u8; 64]; MAX_VERIFIER_IC_POINTS],
}

impl VerifierConfig {
    pub const LEN: usize =
        2 + // version
        1 + // circuit enum
        1 + // padding0
        2 + // vk_len_ic
        2 + // padding1
        (32 * 2) + // alpha
        (32 * 4) + // beta
        (32 * 4) + // gamma
        (32 * 4) + // delta
        (64 * MAX_VERIFIER_IC_POINTS); // ic points
}

#[account(zero_copy)]
#[repr(C)]
pub struct LeafChunk {
    pub mint: Pubkey,
    pub chunk_index: u32,
    pub count: u16,
    pub _padding: [u8; 2], // Explicit padding for alignment
    pub leaves: [[u8; 32]; LEAF_CHUNK_SIZE],
}

impl LeafChunk {
    // Size of data excluding the 8-byte discriminator
    pub const LEN: usize = 32 + 4 + 2 + 2 + (32 * LEAF_CHUNK_SIZE);
}
