use anchor_lang::prelude::*;

// ============================================================================
// Epoch-Based Events
// ============================================================================

#[event]
pub struct DepositEvent {
    pub pool: Pubkey,
    pub epoch: u64,
    pub leaf_index: u64,
    pub commitment: [u8; 32],
    pub encrypted_note: Vec<u8>,
    pub timestamp: i64,
}

#[event]
pub struct WithdrawEvent {
    pub pool: Pubkey,
    pub epoch: u64,
    pub nullifier: [u8; 32],
    pub amount: u64,
    pub recipient: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct TransferEvent {
    pub pool: Pubkey,
    pub spend_epoch: u64,
    pub nullifiers: Vec<[u8; 32]>,
    pub deposit_epoch: u64,
    pub commitments: Vec<[u8; 32]>,
    pub encrypted_notes: Vec<Vec<u8>>,
    pub timestamp: i64,
}

#[event]
pub struct RenewEvent {
    pub pool: Pubkey,
    pub old_epoch: u64,
    pub nullifier: [u8; 32],
    pub new_epoch: u64,
    pub new_commitment: [u8; 32],
    pub encrypted_note: Vec<u8>,
    pub timestamp: i64,
}

#[event]
pub struct EpochRolloverEvent {
    pub pool: Pubkey,
    pub old_epoch: u64,
    pub new_epoch: u64,
    pub slot: u64,
}

#[event]
pub struct EpochFinalizedEvent {
    pub pool: Pubkey,
    pub epoch: u64,
    pub final_root: [u8; 32],
    pub slot: u64,
}

#[event]
pub struct EpochGarbageCollectedEvent {
    pub pool: Pubkey,
    pub epoch: u64,
    pub account_type: String,
    pub slot: u64,
}

#[event]
pub struct PoolPausedEvent {
    pub pool: Pubkey,
    pub paused: bool,
    pub slot: u64,
}

// ============================================================================
// Legacy Events (preserved for compatibility)
// ============================================================================

#[event]
pub struct DepositEventV1 {
    pub version: u8,
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
    pub cm: [u8; 32],
    pub leaf_index: u64,
    pub new_root: [u8; 32],
    pub tx_anchor: [u8; 32],
    pub tag: [u8; 16],
    pub encrypted_note: Vec<u8>,
}

#[event]
pub struct ShieldedTransferEventV1 {
    pub version: u8,
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
    pub root_prev: [u8; 32],
    pub new_root: [u8; 32],
    pub tx_anchor: [u8; 32],
    pub n_in: u8,
    pub n_out: u8,
    pub nf_in: Vec<[u8; 32]>,
    pub cm_out: Vec<[u8; 32]>,
    pub leaf_index_out: Vec<u64>,
    pub tags_out: Vec<[u8; 16]>,
    pub enc_notes: Vec<Vec<u8>>,
}

#[event]
pub struct WithdrawEventV1 {
    pub version: u8,
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
    pub root_prev: [u8; 32],
    pub new_root: [u8; 32],
    pub tx_anchor: [u8; 32],
    pub n_in: u8,
    pub nf_in: Vec<[u8; 32]>,
    pub value: u64,
    pub recipient: Pubkey,
}
