use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::TransferEvent;
use crate::merkle::insert_leaf_epoch;
use crate::verifier::{Groth16Proof, VerifyingKey, verify_groth16_proof};

/// Public inputs for transfer circuit
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TransferPublicInputs {
    pub root: [u8; 32],
    pub nullifier1: [u8; 32],
    pub nullifier2: [u8; 32],
    pub output_commitment1: [u8; 32],
    pub output_commitment2: [u8; 32],
    pub epoch: u64,
    pub tx_anchor: [u8; 32],
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
}

#[derive(Accounts)]
#[instruction(proof_bytes: Vec<u8>, public_inputs: TransferPublicInputs)]
pub struct TransferV2<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Epoch tree being spent from (must be Finalized)
    #[account(
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &public_inputs.epoch.to_le_bytes()],
        bump = spend_epoch_tree.load()?.bump,
    )]
    pub spend_epoch_tree: AccountLoader<'info, EpochTree>,

    /// Current active epoch tree for new commitments
    #[account(
        mut,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &pool_config.current_epoch.to_le_bytes()],
        bump = deposit_epoch_tree.load()?.bump,
    )]
    pub deposit_epoch_tree: AccountLoader<'info, EpochTree>,

    /// Nullifier marker 1
    #[account(
        init,
        payer = payer,
        space = NullifierMarker::LEN,
        seeds = [
            b"nullifier",
            pool_config.key().as_ref(),
            &public_inputs.epoch.to_le_bytes(),
            &public_inputs.nullifier1
        ],
        bump
    )]
    pub nullifier_marker1: Account<'info, NullifierMarker>,

    /// Nullifier marker 2
    #[account(
        init,
        payer = payer,
        space = NullifierMarker::LEN,
        seeds = [
            b"nullifier",
            pool_config.key().as_ref(),
            &public_inputs.epoch.to_le_bytes(),
            &public_inputs.nullifier2
        ],
        bump
    )]
    pub nullifier_marker2: Account<'info, NullifierMarker>,

    /// Leaf chunk for new commitments
    #[account(
        mut,
        seeds = [
            b"leaves",
            pool_config.key().as_ref(),
            &pool_config.current_epoch.to_le_bytes(),
            &(deposit_epoch_tree.load()?.next_index as u32 / LEAF_CHUNK_SIZE as u32).to_le_bytes()
        ],
        bump,
    )]
    pub deposit_leaf_chunk: AccountLoader<'info, EpochLeafChunk>,

    /// Verifier config for transfer circuit
    #[account(
        seeds = [b"verifier", pool_config.key().as_ref(), CircuitType::Transfer.seed()],
        bump,
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<TransferV2>,
    proof_bytes: Vec<u8>,
    public_inputs: TransferPublicInputs,
    encrypted_notes: Vec<Vec<u8>>,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate pool is not paused
    require!(!pool_config.paused, ShieldedPoolError::PoolPaused);

    // Validate spend epoch is finalized and not expired
    {
        let spend_tree = ctx.accounts.spend_epoch_tree.load()?;
        require!(
            spend_tree.get_state() == EpochState::Finalized,
            ShieldedPoolError::EpochNotFinalized
        );

        let expiry_slot = spend_tree.finalized_slot
            .checked_add(pool_config.expiry_slots)
            .ok_or(ShieldedPoolError::MathOverflow)?;
        
        require!(
            clock.slot < expiry_slot,
            ShieldedPoolError::EpochExpired
        );

        // Validate root matches finalized root
        require!(
            public_inputs.root == spend_tree.final_root,
            ShieldedPoolError::InvalidRoot
        );
    }

    // Validate deposit epoch is active
    {
        let deposit_tree = ctx.accounts.deposit_epoch_tree.load()?;
        require!(
            deposit_tree.get_state() == EpochState::Active,
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
    let public_inputs_array = transfer_public_inputs_to_field_elements(&public_inputs);
    let is_valid = verify_groth16_proof(&proof, &public_inputs_array, &vk)?;
    require!(is_valid, ShieldedPoolError::InvalidProof);

    // Initialize nullifier markers
    let nullifier_marker1 = &mut ctx.accounts.nullifier_marker1;
    nullifier_marker1.pool = pool_config.key();
    nullifier_marker1.epoch = public_inputs.epoch;
    nullifier_marker1.nullifier = public_inputs.nullifier1;
    nullifier_marker1.bump = ctx.bumps.nullifier_marker1;

    let nullifier_marker2 = &mut ctx.accounts.nullifier_marker2;
    nullifier_marker2.pool = pool_config.key();
    nullifier_marker2.epoch = public_inputs.epoch;
    nullifier_marker2.nullifier = public_inputs.nullifier2;
    nullifier_marker2.bump = ctx.bumps.nullifier_marker2;

    // Insert output commitments into deposit epoch tree
    let deposit_epoch = pool_config.current_epoch;
    let (leaf_index1, leaf_index2) = {
        let mut deposit_tree = ctx.accounts.deposit_epoch_tree.load_mut()?;
        
        let (idx1, _) = insert_leaf_epoch(&mut deposit_tree, public_inputs.output_commitment1)?;
        let (idx2, _) = insert_leaf_epoch(&mut deposit_tree, public_inputs.output_commitment2)?;
        
        (idx1, idx2)
    };

    // Store commitments in leaf chunk
    {
        let mut chunk = ctx.accounts.deposit_leaf_chunk.load_mut()?;
        let index_in_chunk1 = (leaf_index1 as usize) % LEAF_CHUNK_SIZE;
        let index_in_chunk2 = (leaf_index2 as usize) % LEAF_CHUNK_SIZE;
        
        chunk.leaves[index_in_chunk1] = public_inputs.output_commitment1;
        chunk.leaves[index_in_chunk2] = public_inputs.output_commitment2;
        chunk.count = chunk.count.saturating_add(2);
    }

    emit!(TransferEvent {
        pool: pool_config.key(),
        spend_epoch: public_inputs.epoch,
        nullifiers: vec![public_inputs.nullifier1, public_inputs.nullifier2],
        deposit_epoch,
        commitments: vec![public_inputs.output_commitment1, public_inputs.output_commitment2],
        encrypted_notes,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Transfer: spend_epoch={}, deposit_epoch={}, leaf_indices=[{}, {}]",
        public_inputs.epoch,
        deposit_epoch,
        leaf_index1,
        leaf_index2
    );

    Ok(())
}

/// Convert transfer public inputs to field elements for proof verification
fn transfer_public_inputs_to_field_elements(inputs: &TransferPublicInputs) -> Vec<[u8; 32]> {
    let mut elements = Vec::with_capacity(9);
    
    elements.push(inputs.root);
    elements.push(inputs.nullifier1);
    elements.push(inputs.nullifier2);
    elements.push(inputs.output_commitment1);
    elements.push(inputs.output_commitment2);
    
    // Convert epoch to 32-byte field element
    let mut epoch_bytes = [0u8; 32];
    epoch_bytes[..8].copy_from_slice(&inputs.epoch.to_le_bytes());
    elements.push(epoch_bytes);
    
    elements.push(inputs.tx_anchor);
    elements.push(inputs.pool_id);
    elements.push(inputs.chain_id);
    
    elements
}
