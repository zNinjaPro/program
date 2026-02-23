use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::merkle::compute_zero_hashes;

#[derive(Accounts)]
pub struct InitializePoolV2<'info> {
    #[account(
        init,
        payer = payer,
        space = PoolConfig::LEN,
        seeds = [b"pool_config", mint.key().as_ref()],
        bump
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        init,
        payer = payer,
        space = 8 + EpochTree::LEN,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &0u64.to_le_bytes()],
        bump
    )]
    pub epoch_tree: AccountLoader<'info, EpochTree>,

    /// CHECK: Vault authority PDA - will be owner of vault token account
    #[account(
        seeds = [b"vault_authority", pool_config.key().as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    /// Token vault to hold deposited tokens
    #[account(
        init,
        payer = payer,
        token::mint = mint,
        token::authority = vault_authority,
        seeds = [b"vault", pool_config.key().as_ref()],
        bump
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    /// Token mint for this pool
    pub mint: InterfaceAccount<'info, Mint>,

    /// Authority that can pause pool and transfer authority
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(
    ctx: Context<InitializePoolV2>,
    epoch_duration_slots: u64,
    expiry_slots: u64,
    finalization_delay_slots: u64,
    burn_rate_bps: u16,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate burn rate (max 10% = 1000 basis points)
    require!(burn_rate_bps <= 1000, ShieldedPoolError::InvalidBurnRate);

    // Use defaults if zero is passed
    let epoch_duration = if epoch_duration_slots == 0 {
        DEFAULT_EPOCH_DURATION_SLOTS
    } else {
        epoch_duration_slots
    };

    let expiry = if expiry_slots == 0 {
        DEFAULT_EPOCH_EXPIRY_SLOTS
    } else {
        expiry_slots
    };

    let finalization_delay = if finalization_delay_slots == 0 {
        DEFAULT_FINALIZATION_DELAY_SLOTS
    } else {
        finalization_delay_slots
    };

    // Initialize PoolConfig
    pool_config.version = 2;
    pool_config.mint = ctx.accounts.mint.key();
    pool_config.vault_authority = ctx.accounts.vault_authority.key();
    pool_config.vault = ctx.accounts.vault.key();
    pool_config.authority = ctx.accounts.authority.key();
    pool_config.current_epoch = 0;
    pool_config.epoch_start_slot = clock.slot;
    pool_config.epoch_duration_slots = epoch_duration;
    pool_config.expiry_slots = expiry;
    pool_config.finalization_delay_slots = finalization_delay;
    pool_config.total_deposits = 0;
    pool_config.total_withdrawals = 0;
    pool_config.vault_authority_bump = ctx.bumps.vault_authority;
    pool_config.config_bump = ctx.bumps.pool_config;
    pool_config.paused = false;
    pool_config.burn_rate_bps = burn_rate_bps;
    pool_config.total_burned = 0;
    pool_config.reserved = [0; 46];

    // Initialize EpochTree for epoch 0
    let mut epoch_tree = ctx.accounts.epoch_tree.load_init()?;
    epoch_tree.pool = ctx.accounts.pool_config.key().to_bytes();
    epoch_tree.epoch = 0;
    epoch_tree.start_slot = clock.slot;
    epoch_tree.end_slot = 0;
    epoch_tree.finalized_slot = 0;
    epoch_tree.depth = MERKLE_DEPTH as u8;
    epoch_tree.set_state(EpochState::Active);
    epoch_tree.bump = ctx.bumps.epoch_tree;
    epoch_tree.next_index = 0;
    epoch_tree.frontier = [[0; 32]; MERKLE_DEPTH];
    epoch_tree.roots = [[0; 32]; ROOT_HISTORY];
    epoch_tree.roots_len = 0;
    epoch_tree.roots_head = 0;
    epoch_tree.final_root = [0; 32];
    epoch_tree.zero_hashes = compute_zero_hashes(MERKLE_DEPTH)?;

    msg!("Initialized pool for mint {} with epoch 0", ctx.accounts.mint.key());
    msg!("Epoch duration: {} slots, Expiry: {} slots", epoch_duration, expiry);

    Ok(())
}
