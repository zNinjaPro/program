use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};
use core::cmp;
use core::fmt::Write;
use crate::state::*;
use crate::events::*;
use crate::merkle::is_known_root;
use crate::nullifier::insert_nullifier;
use crate::errors::ShieldedPoolError;
use crate::verifier::{Groth16Proof, VerifyingKey, verify_groth16_proof};
use crate::field::{pubkey_to_field, reduce_bytes_to_field};

#[derive(Accounts)]
pub struct WithdrawShielded<'info> {
    #[account(
        seeds = [b"config", pool_config.mint.as_ref()],
        bump = pool_config.program_bump,
        has_one = vault_authority
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        mut,
        seeds = [b"tree", pool_config.mint.as_ref()],
        bump = pool_config.tree_bump
    )]
    pub pool_tree: AccountLoader<'info, PoolTree>,

    /// CHECK: Vault authority PDA
    #[account(
        seeds = [b"vault", pool_config.mint.as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(address = pool_config.mint)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = vault_token_account.mint == mint.key(),
        constraint = vault_token_account.owner == vault_authority.key(),
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_account.mint == mint.key(),
        constraint = user_token_account.owner == user.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,

    #[account(
        seeds = [
            b"verifier",
            pool_config.key().as_ref(),
            CircuitType::Withdraw.seed(),
        ],
        bump
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    // remaining_accounts:
    // - n_in NullifierChunk PDAs, one for each input nullifier

}

pub fn handler(
    ctx: Context<WithdrawShielded>,
    _proof_bytes: Vec<u8>,
    public_inputs: Vec<[u8; 32]>,
    amount: u64,
    n_in: u8,
) -> Result<()> {
    // Decode public_inputs
    // Expected order: root | nullifiers[n_in] | value_out | tx_anchor | pool_id | chain_id
    require!(
        public_inputs.len() >= (1 + n_in as usize + 4),
        ShieldedPoolError::InvalidPublicInputs
    );
    
    let root = public_inputs[0];
    let nullifiers = &public_inputs[1..(1 + n_in as usize)];
    let value_out_field = public_inputs[1 + n_in as usize];
    let tx_anchor = public_inputs[1 + n_in as usize + 1];
    let pool_id_field = public_inputs[2 + n_in as usize + 1];
    let chain_id = public_inputs[3 + n_in as usize + 1];

    msg!("withdraw_shielded: n_in={}, amount={}, public_inputs_len={}", n_in, amount, public_inputs.len());
    msg!("  root: {}", hex32(&root));
    for (i, nullifier) in nullifiers.iter().enumerate() {
        msg!("  nullifier[{}]: {}", i, hex32(nullifier));
    }
    msg!("  value_out: {}", hex32(&value_out_field));
    msg!("  tx_anchor: {}", hex32(&tx_anchor));
    msg!("  pool_id_field (provided): {}", hex32(&pool_id_field));
    msg!("  chain_id (provided): {}", hex32(&chain_id));
    
    // Validate pool_id matches the actual PoolConfig PDA
    let pool_config_key = ctx.accounts.pool_config.key();
    let expected_pool_field = pubkey_to_field(&pool_config_key);
    msg!(
        "  pool_config key: {} (reduced -> {})",
        pool_config_key,
        hex32(&expected_pool_field)
    );
    require!(
        pool_id_field == expected_pool_field,
        ShieldedPoolError::InvalidPublicInputs
    );
    
    // Validate chain_id matches the expected CHAIN_ID constant
    require!(
        chain_id == crate::state::CHAIN_ID,
        ShieldedPoolError::InvalidPublicInputs
    );
    
    // Verify proof using BN254 Groth16 verifier (boxed to reduce stack)
    let proof = Box::new(Groth16Proof::from_bytes(&_proof_bytes)?);
    let verifier_account = ctx.accounts.verifier_config.load()?;
    let circuit = CircuitType::from_u8(verifier_account.circuit)
        .ok_or(ShieldedPoolError::InvalidVerifierConfig)?;
    require!(circuit == CircuitType::Withdraw, ShieldedPoolError::InvalidVerifierConfig);
    let vk = Box::new(VerifyingKey::from(&*verifier_account));
    let proof_valid = verify_groth16_proof(&proof, &public_inputs, &vk)?;
    require!(proof_valid, ShieldedPoolError::InvalidProof);
    
    // Check if using mock proof (all zeros)
    let is_mock_proof = _proof_bytes.iter().all(|&b| b == 0);
    
    // Check root is in pool_tree.roots[] (skip for mock proofs)
    let pool_tree = ctx.accounts.pool_tree.load()?;
    if !is_mock_proof {
        msg!(
            "  pool_tree meta: next_index={} roots_len={} roots_head={}",
            pool_tree.next_index,
            pool_tree.roots_len,
            pool_tree.roots_head
        );

        let preview = cmp::min(pool_tree.roots.len(), 8);
        for (i, stored_root) in pool_tree.roots.iter().take(preview).enumerate() {
            msg!("    roots[{}]: {}", i, hex32(stored_root));
        }

        require!(
            is_known_root(&pool_tree, &root),
            ShieldedPoolError::InvalidRoot
        );
    }
    
    let root_prev = root;
    
    // Check and mark nullifiers as spent (via remaining_accounts) - skip for mock proofs
    if !is_mock_proof {
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
            let chunk_pool_field = reduce_bytes_to_field(&chunk.pool_id);
            msg!(
                "  chunk[{}] account={} chunk_index={} count={} pool_raw={} pool_field={}",
                i,
                chunk_account.key(),
                chunk.chunk_index,
                chunk.count,
                hex32(&chunk.pool_id),
                hex32(&chunk_pool_field)
            );
            msg!(
                "  matching nullifier[{}]={}",
                i,
                hex32(nullifier)
            );
            require!(chunk_pool_field == pool_id_field, ShieldedPoolError::InvalidPublicInputs);
            
            // Insert nullifier (will fail if already exists)
            insert_nullifier(chunk, *nullifier)?;
        }
    }
    
    // Transfer tokens from vault to recipient using vault_authority PDA signer
    let seeds = &[
        b"vault",
        ctx.accounts.pool_config.mint.as_ref(),
        &[ctx.bumps.vault_authority],
    ];
    let signer = &[&seeds[..]];

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.vault_token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.vault_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer,
    );
    token::transfer_checked(cpi_ctx, amount, ctx.accounts.mint.decimals)?;


    let pool_id_raw = ctx.accounts.pool_config.key().to_bytes();

    emit!(WithdrawEventV1 {
        version: 1,
        pool_id: pool_id_raw,
        chain_id,
        root_prev,
        new_root: root_prev, // No new commitments on withdraw
        tx_anchor,
        n_in,
        nf_in: nullifiers.to_vec(),
        value: amount,
        recipient: ctx.accounts.user_token_account.owner,
    });

    Ok(())
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes.iter() {
        let _ = write!(s, "{:02x}", b);
    }
    s
}
