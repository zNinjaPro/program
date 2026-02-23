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

    // ========================================================================
    // Legacy v1 Instructions — gated behind `legacy-v1` feature
    // ========================================================================

    #[cfg(feature = "legacy-v1")]
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        config_seed: Vec<u8>,
        merkle_depth: u8,
        root_history: u16,
        nullifier_chunk_size: u16,
    ) -> Result<()> {
        instructions::initialize_pool::handler(ctx, config_seed, merkle_depth, root_history, nullifier_chunk_size)
    }

    #[cfg(feature = "legacy-v1")]
    pub fn initialize_leaf_chunk(
        ctx: Context<InitializeLeafChunk>,
        chunk_index: u32,
    ) -> Result<()> {
        instructions::initialize_leaf_chunk::handler(ctx, chunk_index)
    }

    #[cfg(feature = "legacy-v1")]
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

    #[cfg(feature = "legacy-v1")]
    pub fn deposit_shielded(
        ctx: Context<DepositShielded>,
        amount: u64,
        commitment: [u8; 32],
        encrypted_note: Vec<u8>,
        tag: [u8; 16],
    ) -> Result<()> {
        instructions::deposit_shielded::handler(ctx, amount, commitment, encrypted_note, tag)
    }

    #[cfg(feature = "legacy-v1")]
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

    #[cfg(feature = "legacy-v1")]
    pub fn withdraw_shielded(
        ctx: Context<WithdrawShielded>,
        proof_bytes: Vec<u8>,
        public_inputs: Vec<[u8; 32]>,
        amount: u64,
        n_in: u8,
    ) -> Result<()> {
        instructions::withdraw_shielded::handler(ctx, proof_bytes, public_inputs, amount, n_in)
    }

    // ========================================================================
    // Epoch-Based v2 Instructions
    // ========================================================================

    /// Initialize a new epoch-based shielded pool
    /// burn_rate_bps: Burn rate in basis points (10 = 0.1%, max 1000 = 10%)
    pub fn initialize_pool_v2(
        ctx: Context<InitializePoolV2>,
        epoch_duration_slots: u64,
        expiry_slots: u64,
        finalization_delay_slots: u64,
        burn_rate_bps: u16,
    ) -> Result<()> {
        instructions::initialize_pool_v2::handler(ctx, epoch_duration_slots, expiry_slots, finalization_delay_slots, burn_rate_bps)
    }

    /// Initialize a leaf chunk for storing commitments
    pub fn initialize_epoch_leaf_chunk(
        ctx: Context<InitializeEpochLeafChunk>,
        epoch: u64,
        chunk_index: u32,
    ) -> Result<()> {
        instructions::deposit_v2::handler_init_leaf_chunk(ctx, epoch, chunk_index)
    }

    /// Roll over to a new epoch
    pub fn rollover_epoch(ctx: Context<RolloverEpoch>) -> Result<()> {
        instructions::rollover_epoch::handler(ctx)
    }

    /// Finalize a frozen epoch
    pub fn finalize_epoch(ctx: Context<FinalizeEpoch>, epoch: u64) -> Result<()> {
        instructions::finalize_epoch::handler(ctx, epoch)
    }

    /// Deposit tokens into the current epoch
    pub fn deposit_v2(
        ctx: Context<DepositV2>,
        commitment: [u8; 32],
        amount: u64,
        encrypted_note: Vec<u8>,
    ) -> Result<()> {
        instructions::deposit_v2::handler(ctx, commitment, amount, encrypted_note)
    }

    /// Withdraw tokens using a ZK proof
    pub fn withdraw_v2(
        ctx: Context<WithdrawV2>,
        proof_bytes: Vec<u8>,
        public_inputs: WithdrawPublicInputs,
    ) -> Result<()> {
        instructions::withdraw_v2::handler(ctx, proof_bytes, public_inputs)
    }

    /// Private transfer between notes
    pub fn transfer_v2(
        ctx: Context<TransferV2>,
        proof_bytes: Vec<u8>,
        public_inputs: TransferPublicInputs,
        encrypted_notes: Vec<Vec<u8>>,
    ) -> Result<()> {
        instructions::transfer_v2::handler(ctx, proof_bytes, public_inputs, encrypted_notes)
    }

    /// Renew a note by moving it to the current epoch
    pub fn renew_note(
        ctx: Context<RenewNote>,
        proof_bytes: Vec<u8>,
        public_inputs: RenewPublicInputs,
        encrypted_note: Vec<u8>,
    ) -> Result<()> {
        instructions::renew_note::handler(ctx, proof_bytes, public_inputs, encrypted_note)
    }

    // ========================================================================
    // Garbage Collection Instructions
    // ========================================================================

    /// Garbage collect an expired epoch tree
    pub fn gc_epoch_tree(ctx: Context<GarbageCollectEpochTree>, epoch: u64) -> Result<()> {
        instructions::garbage_collect::handler_gc_epoch_tree(ctx, epoch)
    }

    /// Garbage collect an expired leaf chunk
    pub fn gc_leaf_chunk(
        ctx: Context<GarbageCollectLeafChunk>,
        epoch: u64,
        chunk_index: u32,
    ) -> Result<()> {
        instructions::garbage_collect::handler_gc_leaf_chunk(ctx, epoch, chunk_index)
    }

    /// Garbage collect a single nullifier marker
    pub fn gc_nullifier(
        ctx: Context<GarbageCollectNullifier>,
        epoch: u64,
        nullifier: [u8; 32],
    ) -> Result<()> {
        instructions::garbage_collect::handler_gc_nullifier(ctx, epoch, nullifier)
    }

    /// Batch garbage collect multiple nullifier markers
    pub fn gc_nullifier_batch(
        ctx: Context<GarbageCollectNullifierBatch>,
        epoch: u64,
    ) -> Result<()> {
        instructions::garbage_collect::handler_gc_nullifier_batch(ctx, epoch)
    }

    // ========================================================================
    // Admin Instructions
    // ========================================================================

    /// Pause the pool (emergency stop)
    pub fn pause_pool(ctx: Context<PausePool>) -> Result<()> {
        instructions::admin::handler_pause(ctx)
    }

    /// Unpause the pool
    pub fn unpause_pool(ctx: Context<UnpausePool>) -> Result<()> {
        instructions::admin::handler_unpause(ctx)
    }

    /// Transfer pool authority to a new address
    pub fn transfer_authority(ctx: Context<TransferAuthority>) -> Result<()> {
        instructions::admin::handler_transfer_authority(ctx)
    }

    // ========================================================================
    // Utility Instructions — dev-only, gated behind `legacy-v1`
    // ========================================================================

    // Lightweight dev-only method to exercise pairing syscall path.
    // Accepts a raw byte array and runs verify_alt_bn128_pairing.
    #[cfg(feature = "legacy-v1")]
    pub fn verify_pairing(_ctx: Context<VerifyPairing>, input: Vec<u8>) -> Result<()> {
        let ok = crate::syscalls::verify_alt_bn128_pairing(&input)?;
        if ok { msg!("verify_pairing: pairing result = true"); } else { msg!("verify_pairing: pairing result = false"); }
        Ok(())
    }
}

#[cfg(feature = "legacy-v1")]
#[derive(Accounts)]
pub struct VerifyPairing<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
}
