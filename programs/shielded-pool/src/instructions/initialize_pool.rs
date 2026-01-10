use anchor_lang::prelude::*;
use crate::state::*;
use crate::merkle::compute_zero_hashes;

#[derive(Accounts)]
#[instruction(config_seed: Vec<u8>)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = payer,
        space = PoolConfig::LEN,
        seeds = [b"config", mint.key().as_ref()],
        bump
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        init,
        payer = payer,
        space = 8 + PoolTree::LEN,
        seeds = [b"tree", mint.key().as_ref()],
        bump
    )]
    pub pool_tree: AccountLoader<'info, PoolTree>,

    /// CHECK: Vault authority PDA
    #[account(
        seeds = [b"vault", mint.key().as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    /// CHECK: Token-2022 mint
    pub mint: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializePool>,
    _config_seed: Vec<u8>,
    merkle_depth: u8,
    root_history: u16,
    nullifier_chunk_size: u16,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let mut pool_tree = ctx.accounts.pool_tree.load_init()?;

    // Initialize PoolConfig
    pool_config.version = 1;
    pool_config.mint = ctx.accounts.mint.key();
    pool_config.vault_authority = ctx.accounts.vault_authority.key();
    pool_config.merkle_depth = merkle_depth;
    pool_config.root_history = root_history;
    pool_config.program_bump = ctx.bumps.pool_config;
    pool_config.tree_bump = ctx.bumps.pool_tree;
    pool_config.nullifier_chunk_size = nullifier_chunk_size;
    pool_config.reserved = [0; 16];

    // Initialize PoolTree with zero hashes
    pool_tree.depth = merkle_depth;
    pool_tree.next_index = 0;
    pool_tree.frontier = [[0; 32]; MERKLE_DEPTH];
    pool_tree.roots = [[0; 32]; ROOT_HISTORY];
    pool_tree.roots_len = 0;
    pool_tree.roots_head = 0;
    pool_tree.zero_hashes = compute_zero_hashes(merkle_depth as usize);

    Ok(())
}
