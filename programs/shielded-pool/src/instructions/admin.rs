use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::PoolPausedEvent;

// ============================================================================
// Pause Pool
// ============================================================================

#[derive(Accounts)]
pub struct PausePool<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
        constraint = pool_config.authority == authority.key() @ ShieldedPoolError::Unauthorized,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    pub authority: Signer<'info>,
}

pub fn handler_pause(ctx: Context<PausePool>) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    pool_config.paused = true;

    emit!(PoolPausedEvent {
        pool: pool_config.key(),
        paused: true,
        slot: clock.slot,
    });

    msg!("Pool paused by authority {}", ctx.accounts.authority.key());

    Ok(())
}

// ============================================================================
// Unpause Pool
// ============================================================================

#[derive(Accounts)]
pub struct UnpausePool<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
        constraint = pool_config.authority == authority.key() @ ShieldedPoolError::Unauthorized,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    pub authority: Signer<'info>,
}

pub fn handler_unpause(ctx: Context<UnpausePool>) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    pool_config.paused = false;

    emit!(PoolPausedEvent {
        pool: pool_config.key(),
        paused: false,
        slot: clock.slot,
    });

    msg!("Pool unpaused by authority {}", ctx.accounts.authority.key());

    Ok(())
}

// ============================================================================
// Transfer Authority
// ============================================================================

#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
        constraint = pool_config.authority == authority.key() @ ShieldedPoolError::Unauthorized,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    pub authority: Signer<'info>,

    /// CHECK: New authority - can be any pubkey (including multisig)
    pub new_authority: UncheckedAccount<'info>,
}

pub fn handler_transfer_authority(ctx: Context<TransferAuthority>) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let old_authority = pool_config.authority;
    let new_authority = ctx.accounts.new_authority.key();

    pool_config.authority = new_authority;

    msg!(
        "Pool authority transferred from {} to {}",
        old_authority,
        new_authority
    );

    Ok(())
}
