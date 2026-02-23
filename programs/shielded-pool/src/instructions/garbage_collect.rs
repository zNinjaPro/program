use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::EpochGarbageCollectedEvent;

// ============================================================================
// Garbage Collect Epoch Tree
// ============================================================================

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct GarbageCollectEpochTree<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Epoch tree to close (must be expired)
    #[account(
        mut,
        close = collector,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &epoch.to_le_bytes()],
        bump = epoch_tree.load()?.bump,
    )]
    pub epoch_tree: AccountLoader<'info, EpochTree>,

    /// Receives rent refund
    #[account(mut)]
    pub collector: Signer<'info>,
}

pub fn handler_gc_epoch_tree(ctx: Context<GarbageCollectEpochTree>, epoch: u64) -> Result<()> {
    let pool_config = &ctx.accounts.pool_config;
    let clock = Clock::get()?;

    let tree = ctx.accounts.epoch_tree.load()?;

    // Validate epoch is finalized
    require!(
        tree.get_state() == EpochState::Finalized,
        ShieldedPoolError::EpochNotFinalized
    );

    // Validate epoch has expired
    let expiry_slot = tree.finalized_slot
        .checked_add(pool_config.expiry_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    require!(
        clock.slot >= expiry_slot,
        ShieldedPoolError::EpochNotExpired
    );

    emit!(EpochGarbageCollectedEvent {
        pool: pool_config.key(),
        epoch,
        account_type: "EpochTree".to_string(),
        slot: clock.slot,
    });

    msg!("Garbage collected epoch tree for epoch {}", epoch);

    Ok(())
}

// ============================================================================
// Garbage Collect Leaf Chunk
// ============================================================================

#[derive(Accounts)]
#[instruction(epoch: u64, chunk_index: u32)]
pub struct GarbageCollectLeafChunk<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Leaf chunk to close
    #[account(
        mut,
        close = collector,
        seeds = [
            b"leaves",
            pool_config.key().as_ref(),
            &epoch.to_le_bytes(),
            &chunk_index.to_le_bytes()
        ],
        bump,
    )]
    pub leaf_chunk: AccountLoader<'info, EpochLeafChunk>,

    /// Receives rent refund
    #[account(mut)]
    pub collector: Signer<'info>,
}

pub fn handler_gc_leaf_chunk(
    ctx: Context<GarbageCollectLeafChunk>,
    epoch: u64,
    _chunk_index: u32,
) -> Result<()> {
    let pool_config = &ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // We need to verify the epoch is expired
    // Since the EpochTree might already be garbage collected,
    // we use a simpler check based on slot math
    
    // Calculate the earliest possible finalization slot for this epoch
    // epoch_start = pool creation slot + (epoch * epoch_duration)
    // This is an approximation since we don't have the exact epoch tree
    
    // For safety, we check that:
    // 1. The epoch is not the current epoch
    // 2. Enough time has passed based on epoch duration + expiry
    
    let chunk = ctx.accounts.leaf_chunk.load()?;
    
    require!(
        chunk.epoch < pool_config.current_epoch,
        ShieldedPoolError::EpochNotExpired
    );
    
    // Calculate minimum expiry slot
    // At minimum, the epoch must have been active, frozen, finalized, and expired
    let min_slots_for_expiry = pool_config.epoch_duration_slots
        .checked_add(pool_config.finalization_delay_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?
        .checked_add(pool_config.expiry_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    let epochs_ago = pool_config.current_epoch
        .checked_sub(chunk.epoch)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    // If enough epochs have passed, allow garbage collection
    let epochs_for_expiry = min_slots_for_expiry / pool_config.epoch_duration_slots;
    
    require!(
        epochs_ago > epochs_for_expiry,
        ShieldedPoolError::EpochNotExpired
    );

    emit!(EpochGarbageCollectedEvent {
        pool: pool_config.key(),
        epoch,
        account_type: "EpochLeafChunk".to_string(),
        slot: clock.slot,
    });

    msg!("Garbage collected leaf chunk {} for epoch {}", _chunk_index, epoch);

    Ok(())
}

// ============================================================================
// Garbage Collect Nullifier Marker
// ============================================================================

#[derive(Accounts)]
#[instruction(epoch: u64, nullifier: [u8; 32])]
pub struct GarbageCollectNullifier<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Nullifier marker to close
    #[account(
        mut,
        close = collector,
        seeds = [
            b"nullifier",
            pool_config.key().as_ref(),
            &epoch.to_le_bytes(),
            &nullifier
        ],
        bump = nullifier_marker.bump,
    )]
    pub nullifier_marker: Account<'info, NullifierMarker>,

    /// Receives rent refund
    #[account(mut)]
    pub collector: Signer<'info>,
}

pub fn handler_gc_nullifier(
    ctx: Context<GarbageCollectNullifier>,
    epoch: u64,
    _nullifier: [u8; 32],
) -> Result<()> {
    let pool_config = &ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Similar expiry check as leaf chunks
    require!(
        epoch < pool_config.current_epoch,
        ShieldedPoolError::EpochNotExpired
    );
    
    let min_slots_for_expiry = pool_config.epoch_duration_slots
        .checked_add(pool_config.finalization_delay_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?
        .checked_add(pool_config.expiry_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    let epochs_ago = pool_config.current_epoch
        .checked_sub(epoch)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    let epochs_for_expiry = min_slots_for_expiry / pool_config.epoch_duration_slots;
    
    require!(
        epochs_ago > epochs_for_expiry,
        ShieldedPoolError::EpochNotExpired
    );

    emit!(EpochGarbageCollectedEvent {
        pool: pool_config.key(),
        epoch,
        account_type: "NullifierMarker".to_string(),
        slot: clock.slot,
    });

    msg!("Garbage collected nullifier marker for epoch {}", epoch);

    Ok(())
}

// ============================================================================
// Batch Garbage Collect Nullifiers
// ============================================================================

#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct GarbageCollectNullifierBatch<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Receives rent refund for all closed accounts
    #[account(mut)]
    pub collector: Signer<'info>,
    
    pub system_program: Program<'info, System>,
    // Remaining accounts are the nullifier markers to close
}

pub fn handler_gc_nullifier_batch(
    ctx: Context<GarbageCollectNullifierBatch>,
    epoch: u64,
) -> Result<()> {
    let pool_config = &ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate epoch is expired
    require!(
        epoch < pool_config.current_epoch,
        ShieldedPoolError::EpochNotExpired
    );
    
    let min_slots_for_expiry = pool_config.epoch_duration_slots
        .checked_add(pool_config.finalization_delay_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?
        .checked_add(pool_config.expiry_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    let epochs_ago = pool_config.current_epoch
        .checked_sub(epoch)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    let epochs_for_expiry = min_slots_for_expiry / pool_config.epoch_duration_slots;
    
    require!(
        epochs_ago > epochs_for_expiry,
        ShieldedPoolError::EpochNotExpired
    );

    // Close all remaining accounts (nullifier markers)
    // SECURITY: validate each account is actually a NullifierMarker for the target epoch
    let nullifier_discriminator = <NullifierMarker as anchor_lang::Discriminator>::DISCRIMINATOR;
    let mut closed_count = 0u32;
    
    for account_info in ctx.remaining_accounts.iter() {
        // Skip if not owned by this program
        if account_info.owner != ctx.program_id {
            continue;
        }

        // Validate account is a NullifierMarker by checking Anchor discriminator
        {
            let data = account_info.try_borrow_data()?;
            if data.len() < NullifierMarker::LEN {
                return Err(ShieldedPoolError::InvalidNullifierMarker.into());
            }
            // First 8 bytes must be the NullifierMarker discriminator
            if data[..8] != *nullifier_discriminator {
                return Err(ShieldedPoolError::InvalidNullifierMarker.into());
            }
            // Bytes 40..48 are the epoch field (after 8 disc + 32 pool)
            let account_epoch = u64::from_le_bytes(
                data[40..48].try_into().map_err(|_| ShieldedPoolError::InvalidNullifierMarker)?
            );
            if account_epoch != epoch {
                return Err(ShieldedPoolError::InvalidNullifierMarker.into());
            }
        }
        
        // Transfer lamports to collector
        let lamports = account_info.lamports();
        
        **account_info.try_borrow_mut_lamports()? = 0;
        **ctx.accounts.collector.try_borrow_mut_lamports()? = ctx
            .accounts
            .collector
            .lamports()
            .checked_add(lamports)
            .ok_or(ShieldedPoolError::MathOverflow)?;
        
        // Zero out data and reassign to system program
        account_info.try_borrow_mut_data()?.fill(0);
        account_info.assign(&anchor_lang::system_program::ID);
        
        closed_count += 1;
    }

    emit!(EpochGarbageCollectedEvent {
        pool: pool_config.key(),
        epoch,
        account_type: format!("NullifierBatch({})", closed_count),
        slot: clock.slot,
    });

    msg!("Batch garbage collected {} nullifier markers for epoch {}", closed_count, epoch);

    Ok(())
}
