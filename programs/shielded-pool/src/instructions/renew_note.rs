use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::RenewEvent;
use crate::merkle::insert_leaf_epoch;
use crate::verifier::{Groth16Proof, VerifyingKey, verify_groth16_proof};

/// Public inputs for renew circuit
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RenewPublicInputs {
    pub old_root: [u8; 32],
    pub nullifier: [u8; 32],
    pub new_commitment: [u8; 32],
    pub old_epoch: u64,
    pub new_epoch: u64,
    pub tx_anchor: [u8; 32],
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
}

#[derive(Accounts)]
#[instruction(proof_bytes: Vec<u8>, public_inputs: RenewPublicInputs)]
pub struct RenewNote<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Old epoch tree (must be Finalized)
    #[account(
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &public_inputs.old_epoch.to_le_bytes()],
        bump = old_epoch_tree.load()?.bump,
    )]
    pub old_epoch_tree: AccountLoader<'info, EpochTree>,

    /// New epoch tree (must be Active - current epoch)
    #[account(
        mut,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &pool_config.current_epoch.to_le_bytes()],
        bump = new_epoch_tree.load()?.bump,
    )]
    pub new_epoch_tree: AccountLoader<'info, EpochTree>,

    /// Nullifier marker for old epoch
    #[account(
        init,
        payer = payer,
        space = NullifierMarker::LEN,
        seeds = [
            b"nullifier",
            pool_config.key().as_ref(),
            &public_inputs.old_epoch.to_le_bytes(),
            &public_inputs.nullifier
        ],
        bump
    )]
    pub nullifier_marker: Account<'info, NullifierMarker>,

    /// Leaf chunk for new commitment
    #[account(
        mut,
        seeds = [
            b"leaves",
            pool_config.key().as_ref(),
            &pool_config.current_epoch.to_le_bytes(),
            &(new_epoch_tree.load()?.next_index as u32 / LEAF_CHUNK_SIZE as u32).to_le_bytes()
        ],
        bump,
    )]
    pub new_leaf_chunk: AccountLoader<'info, EpochLeafChunk>,

    /// Verifier config for renew circuit
    #[account(
        seeds = [b"verifier", pool_config.key().as_ref(), CircuitType::Renew.seed()],
        bump,
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<RenewNote>,
    proof_bytes: Vec<u8>,
    public_inputs: RenewPublicInputs,
    encrypted_note: Vec<u8>,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate pool is not paused
    require!(!pool_config.paused, ShieldedPoolError::PoolPaused);

    // Validate new_epoch matches current epoch
    require!(
        public_inputs.new_epoch == pool_config.current_epoch,
        ShieldedPoolError::InvalidEpoch
    );

    // Validate old epoch is finalized and not expired
    {
        let old_tree = ctx.accounts.old_epoch_tree.load()?;
        require!(
            old_tree.get_state() == EpochState::Finalized,
            ShieldedPoolError::EpochNotFinalized
        );

        let expiry_slot = old_tree.finalized_slot
            .checked_add(pool_config.expiry_slots)
            .ok_or(ShieldedPoolError::MathOverflow)?;
        
        require!(
            clock.slot < expiry_slot,
            ShieldedPoolError::EpochExpired
        );

        // Validate root matches finalized root
        require!(
            public_inputs.old_root == old_tree.final_root,
            ShieldedPoolError::InvalidRoot
        );
    }

    // Validate new epoch is active
    {
        let new_tree = ctx.accounts.new_epoch_tree.load()?;
        require!(
            new_tree.get_state() == EpochState::Active,
            ShieldedPoolError::EpochNotActive
        );
    }

    // Validate pool_id matches
    require!(
        public_inputs.pool_id == pool_config.key().to_bytes(),
        ShieldedPoolError::InvalidPublicInputs
    );

    // Verify Groth16 proof
    let proof = Box::new(Groth16Proof::from_bytes(&proof_bytes)?);
    let verifier_account = ctx.accounts.verifier_config.load()?;
    let vk = Box::new(VerifyingKey::from(&*verifier_account));
    let public_inputs_array = renew_public_inputs_to_field_elements(&public_inputs);
    let is_valid = verify_groth16_proof(&proof, &public_inputs_array, &vk)?;
    require!(is_valid, ShieldedPoolError::InvalidProof);

    // Initialize nullifier marker for old note
    let nullifier_marker = &mut ctx.accounts.nullifier_marker;
    nullifier_marker.pool = pool_config.key();
    nullifier_marker.epoch = public_inputs.old_epoch;
    nullifier_marker.nullifier = public_inputs.nullifier;
    nullifier_marker.bump = ctx.bumps.nullifier_marker;

    // Insert new commitment into new epoch tree
    let leaf_index = {
        let mut new_tree = ctx.accounts.new_epoch_tree.load_mut()?;
        let (idx, _) = insert_leaf_epoch(&mut new_tree, public_inputs.new_commitment)?;
        idx
    };

    // Store commitment in leaf chunk
    {
        let mut chunk = ctx.accounts.new_leaf_chunk.load_mut()?;
        let index_in_chunk = (leaf_index as usize) % LEAF_CHUNK_SIZE;
        chunk.leaves[index_in_chunk] = public_inputs.new_commitment;
        chunk.count = chunk.count.saturating_add(1);
    }

    emit!(RenewEvent {
        pool: pool_config.key(),
        old_epoch: public_inputs.old_epoch,
        nullifier: public_inputs.nullifier,
        new_epoch: public_inputs.new_epoch,
        new_commitment: public_inputs.new_commitment,
        encrypted_note,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Renew: old_epoch={}, new_epoch={}, leaf_index={}",
        public_inputs.old_epoch,
        public_inputs.new_epoch,
        leaf_index
    );

    Ok(())
}

/// Convert renew public inputs to field elements for proof verification
fn renew_public_inputs_to_field_elements(inputs: &RenewPublicInputs) -> Vec<[u8; 32]> {
    let mut elements = Vec::with_capacity(8);
    
    elements.push(inputs.old_root);
    elements.push(inputs.nullifier);
    elements.push(inputs.new_commitment);
    
    // Convert old_epoch to 32-byte field element
    let mut old_epoch_bytes = [0u8; 32];
    old_epoch_bytes[..8].copy_from_slice(&inputs.old_epoch.to_le_bytes());
    elements.push(old_epoch_bytes);
    
    // Convert new_epoch to 32-byte field element
    let mut new_epoch_bytes = [0u8; 32];
    new_epoch_bytes[..8].copy_from_slice(&inputs.new_epoch.to_le_bytes());
    elements.push(new_epoch_bytes);
    
    elements.push(inputs.tx_anchor);
    elements.push(inputs.pool_id);
    elements.push(inputs.chain_id);
    
    elements
}
