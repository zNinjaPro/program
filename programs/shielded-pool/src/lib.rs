use anchor_lang::prelude::*;

declare_id!("C58iVei3DXTL9BSKe5ZpQuJehqLJL1fQjejdnCAdWzV7");

pub mod state;
pub mod instructions;
pub mod events;
pub mod errors;
pub mod merkle;
pub mod nullifier;
pub mod verifier;
pub mod syscalls;
pub mod field;

use instructions::*;
use state::CircuitType;

#[program]
pub mod shielded_pool {
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        config_seed: Vec<u8>,
        merkle_depth: u8,
        root_history: u16,
        nullifier_chunk_size: u16,
    ) -> Result<()> {
        instructions::initialize_pool::handler(ctx, config_seed, merkle_depth, root_history, nullifier_chunk_size)
    }

    pub fn initialize_leaf_chunk(
        ctx: Context<InitializeLeafChunk>,
        chunk_index: u32,
    ) -> Result<()> {
        instructions::initialize_leaf_chunk::handler(ctx, chunk_index)
    }

    pub fn initialize_nullifier_chunk(
        ctx: Context<InitializeNullifierChunk>,
        pool_id: [u8; 32],
        chunk_index: u32,
    ) -> Result<()> {
        instructions::initialize_nullifier_chunk::handler(ctx, pool_id, chunk_index)
    }

    pub fn initialize_verifier(
        ctx: Context<InitializeVerifier>,
        circuit: CircuitType,
        vk_alpha: [[u8; 32]; 2],
        vk_beta: [[u8; 32]; 4],
        vk_gamma: [[u8; 32]; 4],
        vk_delta: [[u8; 32]; 4],
        ic_points: Vec<[[u8; 32]; 2]>,
    ) -> Result<()> {
        instructions::initialize_verifier::handler(ctx, circuit, vk_alpha, vk_beta, vk_gamma, vk_delta, ic_points)
    }

    pub fn append_verifier_ic(
        ctx: Context<AppendVerifierIc>,
        circuit: CircuitType,
        ic_points: Vec<[[u8; 32]; 2]>,
    ) -> Result<()> {
        instructions::append_verifier_ic::handler(ctx, circuit, ic_points)
    }

    pub fn deposit_shielded(
        ctx: Context<DepositShielded>,
        amount: u64,
        commitment: [u8; 32],
        encrypted_note: Vec<u8>,
        tag: [u8; 16],
    ) -> Result<()> {
        instructions::deposit_shielded::handler(ctx, amount, commitment, encrypted_note, tag)
    }

    pub fn shielded_transfer(
        ctx: Context<ShieldedTransfer>,
        proof_bytes: Vec<u8>,
        public_inputs: Vec<[u8; 32]>,
        encrypted_notes_out: Vec<Vec<u8>>,
        tags_out: Vec<[u8; 16]>,
        n_in: u8,
        n_out: u8,
    ) -> Result<()> {
        instructions::shielded_transfer::handler(ctx, proof_bytes, public_inputs, encrypted_notes_out, tags_out, n_in, n_out)
    }

    pub fn withdraw_shielded(
        ctx: Context<WithdrawShielded>,
        proof_bytes: Vec<u8>,
        public_inputs: Vec<[u8; 32]>,
        amount: u64,
        n_in: u8,
    ) -> Result<()> {
        instructions::withdraw_shielded::handler(ctx, proof_bytes, public_inputs, amount, n_in)
    }

    // Lightweight dev-only method to exercise pairing syscall path.
    // Accepts a raw byte array and runs verify_alt_bn128_pairing.
    pub fn verify_pairing(_ctx: Context<VerifyPairing>, input: Vec<u8>) -> Result<()> {
        let ok = crate::syscalls::verify_alt_bn128_pairing(&input)?;
        if ok { msg!("verify_pairing: pairing result = true"); } else { msg!("verify_pairing: pairing result = false"); }
        Ok(())
    }
}

#[derive(Accounts)]
pub struct VerifyPairing<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
}
