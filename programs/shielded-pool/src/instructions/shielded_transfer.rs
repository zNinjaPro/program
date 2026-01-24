use anchor_lang::prelude::*;
use crate::state::*;
use crate::events::*;
use crate::merkle::{insert_leaf, is_known_root};
use crate::nullifier::insert_nullifier;
use crate::errors::ShieldedPoolError;
use crate::verifier::{Groth16Proof, VerifyingKey, verify_groth16_proof};
use crate::field::pubkey_to_field;

#[derive(Accounts)]
pub struct ShieldedTransfer<'info> {
    #[account(
        seeds = [b"config", pool_config.mint.as_ref()],
        bump = pool_config.program_bump
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        mut,
        seeds = [b"tree", pool_config.mint.as_ref()],
        bump = pool_config.tree_bump
    )]
    pub pool_tree: AccountLoader<'info, PoolTree>,

    pub user: Signer<'info>,

    #[account(
        seeds = [
            b"verifier",
            pool_config.key().as_ref(),
            CircuitType::Transfer.seed(),
        ],
        bump
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    // remaining_accounts:
    // - First n_in accounts: NullifierChunk PDAs for each input nullifier
    // - Next accounts: LeafChunk PDAs for each output commitment
}

pub fn handler(
    ctx: Context<ShieldedTransfer>,
    _proof_bytes: Vec<u8>,
    public_inputs: Vec<[u8; 32]>,
    encrypted_notes_out: Vec<Vec<u8>>,
    tags_out: Vec<[u8; 16]>,
    n_in: u8,
    n_out: u8,
) -> Result<()> {
    // Decode public_inputs
    // Canonical order: root | nullifiers[n_in] | commitments[n_out] | tx_anchor | pool_id | chain_id
    // Legacy (currently accepted): root | nullifiers[n_in] | commitments[n_out]
    require!(
        public_inputs.len() >= (1 + n_in as usize + n_out as usize),
        ShieldedPoolError::InvalidPublicInputs
    );
    
    let root = public_inputs[0];
    let nullifiers = &public_inputs[1..(1 + n_in as usize)];
    let commitments = &public_inputs[(1 + n_in as usize)..(1 + n_in as usize + n_out as usize)];
    let extra_offset = 1 + n_in as usize + n_out as usize;
    let have_extras = public_inputs.len() >= extra_offset + 3;
    let (tx_anchor, pool_id_field, chain_id_field) = if have_extras {
        (
            public_inputs[extra_offset],
            public_inputs[extra_offset + 1],
            public_inputs[extra_offset + 2],
        )
    } else {
        ([0u8; 32], [0u8; 32], [0u8; 32])
    };
    let pool_id = ctx.accounts.pool_config.key().to_bytes();
    let chain_id = crate::state::CHAIN_ID;

    #[cfg(feature = "canonical-transfer-pis")]
    {
        require!(have_extras, ShieldedPoolError::InvalidPublicInputs);
    }

    if have_extras {
        let expected_pool_field = pubkey_to_field(&ctx.accounts.pool_config.key());
        require!(pool_id_field == expected_pool_field, ShieldedPoolError::InvalidPublicInputs);
        require!(chain_id_field == chain_id, ShieldedPoolError::InvalidPublicInputs);
    } else {
        msg!("⚠️ Legacy public inputs detected: tx_anchor/pool_id/chain_id not supplied");
    }
    
    // Verify proof using BN254 Groth16 verifier (boxed to reduce stack)
    let proof = Box::new(Groth16Proof::from_bytes(&_proof_bytes)?);
    let verifier_account = ctx.accounts.verifier_config.load()?;
    let circuit = CircuitType::from_u8(verifier_account.circuit)
        .ok_or(ShieldedPoolError::InvalidVerifierConfig)?;
    require!(circuit == CircuitType::Transfer, ShieldedPoolError::InvalidVerifierConfig);
    let vk = Box::new(VerifyingKey::from(&*verifier_account));
    let proof_valid = verify_groth16_proof(&proof, &public_inputs, &vk)?;
    require!(proof_valid, ShieldedPoolError::InvalidProof);
    
    // Check root is in pool_tree.roots[]
    let mut pool_tree = ctx.accounts.pool_tree.load_mut()?;
    require!(
        is_known_root(&pool_tree, &root),
        ShieldedPoolError::InvalidRoot
    );
    
    let root_prev = root;
    
    // Check and mark nullifiers as spent (via remaining_accounts)
    // remaining_accounts should contain NullifierChunk PDAs
    let remaining = &ctx.remaining_accounts;
    require!(
        remaining.len() >= n_in as usize,
        ShieldedPoolError::InvalidPublicInputs
    );
    
    for (i, nullifier) in nullifiers.iter().enumerate() {
        let chunk_account = &remaining[i];
        
        // Access zero-copy NullifierChunk via bytemuck
        let mut data = chunk_account.try_borrow_mut_data()?;
        let (_, chunk_data) = data.split_at_mut(8); // Skip discriminator
        let chunk = bytemuck::from_bytes_mut::<NullifierChunk>(chunk_data);
        
        // Verify chunk belongs to this pool
        require!(chunk.pool_id == pool_id, ShieldedPoolError::InvalidPublicInputs);
        
        // Insert nullifier (will fail if already exists)
        insert_nullifier(chunk, *nullifier)?;
    }
    
    // Append output commitments: write to leaf chunks then update tree frontier
    let mut leaf_indices = Vec::new();
    let mut new_root = root_prev;

    let start_index = pool_tree.next_index as usize;
    for (i, commitment) in commitments.iter().enumerate() {
        let global_index = start_index + i;
        let chunk_index: u32 = (global_index / LEAF_CHUNK_SIZE) as u32;
        let offset: usize = global_index % LEAF_CHUNK_SIZE;

        let (expected_pda, _bump) = Pubkey::find_program_address(
            &[b"leaf", ctx.accounts.pool_config.mint.as_ref(), &chunk_index.to_be_bytes()],
            &crate::ID,
        );

        // Find leaf chunk account in remaining_accounts
        let chunk_account_info = ctx
            .remaining_accounts
            .iter()
            .find(|ai| ai.key() == expected_pda)
            .ok_or(ShieldedPoolError::InvalidPublicInputs)?;

        // Access zero-copy account via bytemuck
        let mut data = chunk_account_info.try_borrow_mut_data()?;
        let (_, chunk_data) = data.split_at_mut(8); // Skip discriminator
        let leaf_chunk = bytemuck::from_bytes_mut::<LeafChunk>(chunk_data);
        
        require!(leaf_chunk.mint == ctx.accounts.pool_config.mint, ShieldedPoolError::InvalidPublicInputs);
        require!(leaf_chunk.chunk_index == chunk_index, ShieldedPoolError::InvalidPublicInputs);

        leaf_chunk.leaves[offset] = *commitment;
        if leaf_chunk.count as usize == offset {
            leaf_chunk.count = leaf_chunk
                .count
                .checked_add(1)
                .ok_or(ShieldedPoolError::MathOverflow)?;
        }
        drop(data); // Release borrow before next iteration

        let (leaf_index, root) = insert_leaf(&mut pool_tree, *commitment)?;
        leaf_indices.push(leaf_index);
        new_root = root;
    }
    
    emit!(ShieldedTransferEventV1 {
        version: 1,
        pool_id,
        chain_id,
        root_prev,
        new_root,
        tx_anchor,
        n_in,
        n_out,
        nf_in: nullifiers.to_vec(),
        cm_out: commitments.to_vec(),
        leaf_index_out: leaf_indices,
        tags_out,
        enc_notes: encrypted_notes_out,
    });

    Ok(())
}
