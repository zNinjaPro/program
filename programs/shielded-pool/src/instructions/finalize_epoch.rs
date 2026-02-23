use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::EpochFinalizedEvent;
use crate::merkle::compute_root_epoch;

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct FinalizeEpoch<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Epoch tree to finalize (must be Frozen)
    #[account(
        mut,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &epoch.to_le_bytes()],
        bump = epoch_tree.load()?.bump,
    )]
    pub epoch_tree: AccountLoader<'info, EpochTree>,
}

pub fn handler(ctx: Context<FinalizeEpoch>, epoch: u64) -> Result<()> {
    let pool_config = &ctx.accounts.pool_config;
    let clock = Clock::get()?;

    let mut tree = ctx.accounts.epoch_tree.load_mut()?;

    // Validate epoch is Frozen
    require!(
        tree.get_state() == EpochState::Frozen,
        ShieldedPoolError::EpochNotFrozen
    );

    // Validate finalization delay has passed
    let finalization_ready_slot = tree.end_slot
        .checked_add(pool_config.finalization_delay_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    require!(
        clock.slot >= finalization_ready_slot,
        ShieldedPoolError::EpochNotReadyForFinalization
    );

    // Compute and commit final root
    let final_root = compute_root_epoch(&tree)?;
    tree.final_root = final_root;
    tree.finalized_slot = clock.slot;
    tree.set_state(EpochState::Finalized);

    emit!(EpochFinalizedEvent {
        pool: pool_config.key(),
        epoch,
        final_root,
        slot: clock.slot,
    });

    msg!("Finalized epoch {} with root {:?}", epoch, &final_root[..8]);

    Ok(())
}
