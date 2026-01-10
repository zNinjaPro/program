use anchor_lang::prelude::*;

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
