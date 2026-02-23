use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};
use crate::state::*;
use crate::events::*;
use crate::merkle::insert_leaf;
use crate::errors::ShieldedPoolError;

#[derive(Accounts)]
pub struct DepositShielded<'info> {
    #[account(
        seeds = [b"config", pool_config.mint.as_ref()],
        bump = pool_config.program_bump,
        has_one = mint
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        mut,
        seeds = [b"tree", pool_config.mint.as_ref()],
        bump = pool_config.tree_bump
    )]
    pub pool_tree: AccountLoader<'info, PoolTree>,

    #[account(address = pool_config.mint)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = user_token_account.mint == mint.key(),
        constraint = user_token_account.owner == user.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_token_account.mint == mint.key(),
        constraint = vault_token_account.owner == pool_config.vault_authority,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(
    ctx: Context<DepositShielded>,
    amount: u64,
    commitment: [u8; 32],
    encrypted_note: Vec<u8>,
    tag: [u8; 16],
) -> Result<()> {
    // Transfer tokens from user to vault using transfer_checked
    let cpi_accounts = TransferChecked {
        from: ctx.accounts.user_token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.vault_token_account.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer_checked(cpi_ctx, amount, ctx.accounts.mint.decimals)?;

    // Determine chunk for this leaf and write commitment into LeafChunk PDA (provided in remaining_accounts)
    let mut pool_tree = ctx.accounts.pool_tree.load_mut()?;
    let next_index = pool_tree.next_index as usize;
    let chunk_index: u32 = (next_index / LEAF_CHUNK_SIZE) as u32;
    let offset: usize = next_index % LEAF_CHUNK_SIZE;

    let (expected_pda, _bump) = Pubkey::find_program_address(
        &[b"leaf", ctx.accounts.pool_config.mint.as_ref(), &chunk_index.to_be_bytes()],
        &crate::ID,
    );

    // Find the provided chunk account
    let chunk_account_info = ctx
        .remaining_accounts
        .iter()
        .find(|ai| ai.key() == expected_pda)
        .ok_or(ShieldedPoolError::InvalidPublicInputs)?;

    // Access zero-copy account directly via bytemuck
    let mut data = chunk_account_info.try_borrow_mut_data()?;
    let (_disc, chunk_data) = data.split_at_mut(8); // Skip 8-byte discriminator
    let leaf_chunk = bytemuck::from_bytes_mut::<LeafChunk>(chunk_data);
    
    if leaf_chunk.mint != ctx.accounts.pool_config.mint {
        return Err(ShieldedPoolError::InvalidPublicInputs.into());
    }
    if leaf_chunk.chunk_index != chunk_index {
        return Err(ShieldedPoolError::InvalidPublicInputs.into());
    }

    leaf_chunk.leaves[offset] = commitment;
    if leaf_chunk.count as usize == offset {
        leaf_chunk.count = leaf_chunk
            .count
            .checked_add(1)
            .ok_or(ShieldedPoolError::MathOverflow)?;
    }
    // Data written back automatically when borrow drops

    // Insert commitment into Merkle tree frontier
    let (leaf_index, new_root) = insert_leaf(&mut pool_tree, commitment)?;

    // Emit deposit event with pool_id (PoolConfig PDA address) and chain_id
    let pool_id = ctx.accounts.pool_config.key().to_bytes();
    emit!(DepositEventV1 {
        version: 1,
        pool_id,
        chain_id: crate::state::CHAIN_ID,
        cm: commitment,
        leaf_index,
        new_root,
        tx_anchor: [0; 32],
        tag,
        encrypted_note,
    });

    Ok(())
}
