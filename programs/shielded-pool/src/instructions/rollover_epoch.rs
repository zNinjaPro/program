use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::EpochRolloverEvent;
use crate::merkle::compute_zero_hashes;

#[derive(Accounts)]
pub struct RolloverEpoch<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Current epoch tree to freeze
    #[account(
        mut,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &pool_config.current_epoch.to_le_bytes()],
        bump = current_epoch_tree.load()?.bump,
    )]
    pub current_epoch_tree: AccountLoader<'info, EpochTree>,

    /// New epoch tree to create
    #[account(
        init,
        payer = payer,
        space = 8 + EpochTree::LEN,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &(pool_config.current_epoch + 1).to_le_bytes()],
        bump
    )]
    pub new_epoch_tree: AccountLoader<'info, EpochTree>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<RolloverEpoch>) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate pool is not paused
    require!(!pool_config.paused, ShieldedPoolError::PoolPaused);

    // Validate epoch is ready for rollover
    let epoch_end_slot = pool_config.epoch_start_slot
        .checked_add(pool_config.epoch_duration_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    require!(
        clock.slot >= epoch_end_slot,
        ShieldedPoolError::EpochNotReadyForRollover
    );

    // Freeze current epoch
    {
        let mut current_tree = ctx.accounts.current_epoch_tree.load_mut()?;
        require!(
            current_tree.get_state() == EpochState::Active,
            ShieldedPoolError::EpochNotActive
        );
        current_tree.set_state(EpochState::Frozen);
        current_tree.end_slot = clock.slot;
    }

    let old_epoch = pool_config.current_epoch;
    let new_epoch = old_epoch + 1;

    // Update pool config
    pool_config.current_epoch = new_epoch;
    pool_config.epoch_start_slot = clock.slot;

    // Initialize new epoch tree
    {
        let mut new_tree = ctx.accounts.new_epoch_tree.load_init()?;
        new_tree.pool = pool_config.key().to_bytes();
        new_tree.epoch = new_epoch;
        new_tree.start_slot = clock.slot;
        new_tree.end_slot = 0;
        new_tree.finalized_slot = 0;
        new_tree.depth = MERKLE_DEPTH as u8;
        new_tree.set_state(EpochState::Active);
        new_tree.bump = ctx.bumps.new_epoch_tree;
        new_tree.next_index = 0;
        new_tree.frontier = [[0; 32]; MERKLE_DEPTH];
        new_tree.roots = [[0; 32]; ROOT_HISTORY];
        new_tree.roots_len = 0;
        new_tree.roots_head = 0;
        new_tree.final_root = [0; 32];
        new_tree.zero_hashes = compute_zero_hashes(MERKLE_DEPTH);
    }

    emit!(EpochRolloverEvent {
        pool: pool_config.key(),
        old_epoch,
        new_epoch,
        slot: clock.slot,
    });

    msg!("Rolled over from epoch {} to epoch {}", old_epoch, new_epoch);

    Ok(())
}
