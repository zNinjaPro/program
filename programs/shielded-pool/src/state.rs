use anchor_lang::prelude::*;

// ============================================================================
// Constants for Epoch-Based Shielded Pool
// ============================================================================

/// Merkle tree depth per epoch (2^12 = 4,096 deposits per epoch)
pub const MERKLE_DEPTH: usize = 12;

/// Number of historical roots to keep per epoch
pub const ROOT_HISTORY: usize = 32;

/// Commitments per leaf chunk PDA
pub const LEAF_CHUNK_SIZE: usize = 256;

/// Maximum nullifiers per chunk (legacy)
pub const MAX_NULLIFIER_CHUNK_SIZE: usize = 256;

/// Maximum IC points for verifier
pub const MAX_VERIFIER_IC_POINTS: usize = 64;

/// Default epoch duration in slots (~2 weeks at 400ms slots)
pub const DEFAULT_EPOCH_DURATION_SLOTS: u64 = 3_024_000;

/// Default grace period before epoch can be garbage collected (~6 months)
pub const DEFAULT_EPOCH_EXPIRY_SLOTS: u64 = 38_880_000;

/// Default finalization delay after epoch ends (~1 day)
pub const DEFAULT_FINALIZATION_DELAY_SLOTS: u64 = 216_000;

// Chain ID for Solana networks
// For devnet/testnet: use a test value
// For mainnet-beta: should be updated to a production value
pub const CHAIN_ID: [u8; 32] = [0u8; 32];

// ============================================================================
// Legacy constants for backward compatibility with PoolTree
// ============================================================================

/// Legacy merkle tree depth (for v1 PoolTree)
pub const LEGACY_MERKLE_DEPTH: usize = 32;

/// Legacy root history size (for v1 PoolTree)
pub const LEGACY_ROOT_HISTORY: usize = 64;

// ============================================================================
// PoolConfig - Global configuration for a shielded pool instance
// ============================================================================

#[account]
pub struct PoolConfig {
    /// Schema version for upgrades (1 = legacy, 2 = epoch-based)
    pub version: u8,

    /// Token mint this pool supports
    pub mint: Pubkey,

    /// PDA authority over the token vault
    pub vault_authority: Pubkey,

    /// Token vault holding all deposited funds
    pub vault: Pubkey,

    /// Authority that can pause/unpause and transfer authority
    pub authority: Pubkey,

    /// Current active epoch number (starts at 0)
    pub current_epoch: u64,

    /// Slot when current epoch started
    pub epoch_start_slot: u64,

    /// Epoch duration in slots
    pub epoch_duration_slots: u64,

    /// Slots after epoch end before garbage collection allowed
    pub expiry_slots: u64,

    /// Finalization delay after epoch ends
    pub finalization_delay_slots: u64,

    /// Total deposits across all epochs (for stats)
    pub total_deposits: u64,

    /// Total withdrawals across all epochs (for stats)
    pub total_withdrawals: u64,

    /// Bump seed for vault authority PDA
    pub vault_authority_bump: u8,

    /// Bump seed for this config PDA
    pub config_bump: u8,

    /// Emergency pause flag
    pub paused: bool,

    // ========== Legacy v1 fields (for backward compatibility) ==========
    /// Legacy: Merkle tree depth
    pub merkle_depth: u8,

    /// Legacy: Root history size
    pub root_history: u16,

    /// Legacy: Program bump (same as config_bump for v1)
    pub program_bump: u8,

    /// Legacy: Tree bump
    pub tree_bump: u8,

    /// Legacy: Nullifier chunk size
    pub nullifier_chunk_size: u16,

    /// Reserved for future use
    pub reserved: [u8; 56],
}

impl PoolConfig {
    pub const LEN: usize = 8 +  // discriminator
        1 +                     // version
        32 +                    // mint
        32 +                    // vault_authority
        32 +                    // vault
        32 +                    // authority
        8 +                     // current_epoch
        8 +                     // epoch_start_slot
        8 +                     // epoch_duration_slots
        8 +                     // expiry_slots
        8 +                     // finalization_delay_slots
        8 +                     // total_deposits
        8 +                     // total_withdrawals
        1 +                     // vault_authority_bump
        1 +                     // config_bump
        1 +                     // paused
        1 +                     // merkle_depth
        2 +                     // root_history
        1 +                     // program_bump
        1 +                     // tree_bump
        2 +                     // nullifier_chunk_size
        56;                     // reserved
}

// ============================================================================
// EpochState - State of an epoch in its lifecycle
// ============================================================================

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EpochState {
    /// Accepting deposits
    #[default]
    Active,
    /// No new deposits, spends still allowed
    Frozen,
    /// Root committed, spends allowed against final_root
    Finalized,
}

// ============================================================================
// EpochTree - Per-epoch Merkle tree state
// ============================================================================

#[account(zero_copy)]
#[repr(C)]
pub struct EpochTree {
    /// Pool this epoch belongs to
    pub pool: [u8; 32],

    /// Epoch number
    pub epoch: u64,

    /// Slot when this epoch started accepting deposits
    pub start_slot: u64,

    /// Slot when this epoch stopped accepting deposits (0 if active)
    pub end_slot: u64,

    /// Slot when this epoch was finalized (0 if not finalized)
    pub finalized_slot: u64,

    /// Merkle tree depth
    pub depth: u8,

    /// Epoch state (0=Active, 1=Frozen, 2=Finalized)
    pub state: u8,

    /// Bump seed for this PDA
    pub bump: u8,

    /// Padding for alignment
    pub _padding0: [u8; 5],

    /// Next leaf index (number of deposits in this epoch)
    pub next_index: u64,

    /// Current frontier nodes for incremental tree updates
    pub frontier: [[u8; 32]; MERKLE_DEPTH],

    /// Ring buffer of historical roots (for proof validity window)
    pub roots: [[u8; 32]; ROOT_HISTORY],

    /// Number of roots in ring buffer
    pub roots_len: u8,

    /// Current head position in ring buffer
    pub roots_head: u8,

    /// Padding for alignment
    pub _padding1: [u8; 6],

    /// Final committed root (set on finalization)
    pub final_root: [u8; 32],

    /// Precomputed zero hashes for empty subtrees
    pub zero_hashes: [[u8; 32]; MERKLE_DEPTH],
}

impl EpochTree {
    pub const LEN: usize = 
        32 +                        // pool
        8 +                         // epoch
        8 +                         // start_slot
        8 +                         // end_slot
        8 +                         // finalized_slot
        1 +                         // depth
        1 +                         // state
        1 +                         // bump
        5 +                         // _padding0
        8 +                         // next_index
        (32 * MERKLE_DEPTH) +       // frontier
        (32 * ROOT_HISTORY) +       // roots
        1 +                         // roots_len
        1 +                         // roots_head
        6 +                         // _padding1
        32 +                        // final_root
        (32 * MERKLE_DEPTH);        // zero_hashes

    pub fn get_state(&self) -> EpochState {
        match self.state {
            0 => EpochState::Active,
            1 => EpochState::Frozen,
            2 => EpochState::Finalized,
            _ => EpochState::Active,
        }
    }

    pub fn set_state(&mut self, state: EpochState) {
        self.state = match state {
            EpochState::Active => 0,
            EpochState::Frozen => 1,
            EpochState::Finalized => 2,
        };
    }
}

// ============================================================================
// EpochLeafChunk - Stores commitments (leaves) for an epoch in chunks
// ============================================================================

#[account(zero_copy)]
#[repr(C)]
pub struct EpochLeafChunk {
    /// Pool this chunk belongs to
    pub pool: [u8; 32],

    /// Epoch number
    pub epoch: u64,

    /// Chunk index within epoch (0, 1, 2, ...)
    pub chunk_index: u32,

    /// Number of leaves in this chunk
    pub count: u16,

    /// Padding for alignment
    pub _padding: [u8; 2],

    /// Leaf commitments (32 bytes each)
    pub leaves: [[u8; 32]; LEAF_CHUNK_SIZE],
}

impl EpochLeafChunk {
    pub const LEN: usize = 
        32 +                        // pool
        8 +                         // epoch
        4 +                         // chunk_index
        2 +                         // count
        2 +                         // _padding
        (32 * LEAF_CHUNK_SIZE);     // leaves
}

// ============================================================================
// NullifierMarker - Individual PDA for each spent nullifier (O(1) lookup)
// ============================================================================

#[account]
pub struct NullifierMarker {
    /// Pool this nullifier belongs to
    pub pool: Pubkey,

    /// Epoch number (nullifiers are epoch-scoped)
    pub epoch: u64,

    /// The nullifier hash
    pub nullifier: [u8; 32],

    /// Bump seed for this PDA
    pub bump: u8,
}

impl NullifierMarker {
    pub const LEN: usize = 8 +  // discriminator
        32 +                    // pool
        8 +                     // epoch
        32 +                    // nullifier
        1;                      // bump
}

// ============================================================================
// CircuitType - Enum for different circuit types
// ============================================================================

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CircuitType {
    Withdraw = 0,
    Transfer = 1,
    Renew = 2,
}

impl CircuitType {
    pub const fn seed(self) -> &'static [u8] {
        match self {
            CircuitType::Withdraw => b"withdraw",
            CircuitType::Transfer => b"transfer",
            CircuitType::Renew => b"renew",
        }
    }

    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CircuitType::Withdraw),
            1 => Some(CircuitType::Transfer),
            2 => Some(CircuitType::Renew),
            _ => None,
        }
    }

    /// For backward compatibility with legacy code using ShieldedTransfer
    pub const fn shielded_transfer() -> Self {
        CircuitType::Transfer
    }
}

// ============================================================================
// VerifierConfig - Stores the Groth16 verification key for a circuit type
// ============================================================================

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

// ============================================================================
// Legacy types preserved for migration (if needed)
// ============================================================================

/// Legacy nullifier chunk - kept for reference during migration
#[account(zero_copy)]
#[repr(C)]
pub struct NullifierChunk {
    pub pool_id: [u8; 32],
    pub chunk_index: u32,
    pub count: u16,
    pub _padding: [u8; 2],
    pub nodes: [[u8; 32]; 256],
}

impl NullifierChunk {
    pub const LEN: usize = 32 + 4 + 2 + 2 + (32 * 256);
}

/// Legacy leaf chunk - kept for reference during migration
#[account(zero_copy)]
#[repr(C)]
pub struct LeafChunk {
    pub mint: Pubkey,
    pub chunk_index: u32,
    pub count: u16,
    pub _padding: [u8; 2],
    pub leaves: [[u8; 32]; 256],
}

impl LeafChunk {
    pub const LEN: usize = 32 + 4 + 2 + 2 + (32 * 256);
}

/// Legacy pool tree - kept for reference during migration
#[account(zero_copy)]
#[repr(C)]
pub struct PoolTree {
    pub depth: u8,
    pub _padding0: [u8; 7],
    pub next_index: u64,
    pub frontier: [[u8; 32]; 32],
    pub roots: [[u8; 32]; 64],
    pub roots_len: u8,
    pub roots_head: u8,
    pub zero_hashes: [[u8; 32]; 32],
    pub _padding_end: [u8; 6],
}

impl PoolTree {
    pub const LEN: usize = 1 + 7 + 8 + (32 * 32) + (32 * 64) + 1 + 1 + (32 * 32) + 6;
}
