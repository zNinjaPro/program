use anchor_lang::prelude::*;
use anchor_spl::token_interface::{burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked};
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::DepositEvent;
use crate::merkle::insert_leaf_epoch;

#[derive(Accounts)]
pub struct DepositV2<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Current active epoch tree
    #[account(
        mut,
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &pool_config.current_epoch.to_le_bytes()],
        bump = epoch_tree.load()?.bump,
    )]
    pub epoch_tree: AccountLoader<'info, EpochTree>,

    /// Leaf chunk for storing commitment
    #[account(
        mut,
        seeds = [
            b"leaves",
            pool_config.key().as_ref(),
            &pool_config.current_epoch.to_le_bytes(),
            &(epoch_tree.load()?.next_index as u32 / LEAF_CHUNK_SIZE as u32).to_le_bytes()
        ],
        bump,
    )]
    pub leaf_chunk: AccountLoader<'info, EpochLeafChunk>,

    /// Token vault
    #[account(
        mut,
        address = pool_config.vault,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    /// Depositor's token account - also serves as gate validation (must have non-zero balance)
    #[account(
        mut,
        constraint = depositor_token_account.mint == pool_config.mint @ ShieldedPoolError::InvalidCommitment,
        constraint = depositor_token_account.owner == depositor.key() @ ShieldedPoolError::Unauthorized,
        constraint = depositor_token_account.amount > 0 @ ShieldedPoolError::InsufficientGateBalance,
    )]
    pub depositor_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Token mint
    #[account(
        address = pool_config.mint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Depositor (must sign)
    pub depositor: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(
    ctx: Context<DepositV2>,
    commitment: [u8; 32],
    amount: u64,
    encrypted_note: Vec<u8>,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate pool is not paused
    require!(!pool_config.paused, ShieldedPoolError::PoolPaused);

    // Validate epoch is still active
    let epoch_end_slot = pool_config.epoch_start_slot
        .checked_add(pool_config.epoch_duration_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    require!(
        clock.slot < epoch_end_slot,
        ShieldedPoolError::EpochNotActive
    );

    // Calculate burn amount (0.1% = 10 basis points by default)
    let burn_amount = amount
        .checked_mul(pool_config.burn_rate_bps as u64)
        .ok_or(ShieldedPoolError::BurnOverflow)?
        .checked_div(10_000)
        .ok_or(ShieldedPoolError::BurnOverflow)?;
    
    let deposit_amount = amount
        .checked_sub(burn_amount)
        .ok_or(ShieldedPoolError::BurnOverflow)?;

    // Burn the burn_amount from depositor's token account
    if burn_amount > 0 {
        let burn_cpi_accounts = Burn {
            mint: ctx.accounts.mint.to_account_info(),
            from: ctx.accounts.depositor_token_account.to_account_info(),
            authority: ctx.accounts.depositor.to_account_info(),
        };
        let burn_cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            burn_cpi_accounts,
        );
        burn(burn_cpi_ctx, burn_amount)?;

        // Update cumulative burn counter
        pool_config.total_burned = pool_config.total_burned.saturating_add(burn_amount);
    }

    // Transfer remaining tokens from depositor to vault
    let cpi_accounts = TransferChecked {
        from: ctx.accounts.depositor_token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
        authority: ctx.accounts.depositor.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
    );
    transfer_checked(cpi_ctx, deposit_amount, ctx.accounts.mint.decimals)?;

    // Insert commitment into epoch tree
    let (leaf_index, _new_root) = {
        let mut tree = ctx.accounts.epoch_tree.load_mut()?;
        require!(
            tree.get_state() == EpochState::Active,
            ShieldedPoolError::EpochNotActive
        );
        insert_leaf_epoch(&mut tree, commitment)?
    };

    // Store commitment in leaf chunk
    {
        let mut chunk = ctx.accounts.leaf_chunk.load_mut()?;
        let index_in_chunk = (leaf_index as usize) % LEAF_CHUNK_SIZE;
        chunk.leaves[index_in_chunk] = commitment;
        chunk.count = chunk.count.saturating_add(1);
    }

    // Update pool stats
    pool_config.total_deposits = pool_config.total_deposits.saturating_add(1);

    emit!(DepositEvent {
        pool: pool_config.key(),
        epoch: pool_config.current_epoch,
        leaf_index,
        commitment,
        encrypted_note,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Deposit: epoch={}, leaf_index={}, amount={}, burned={}, credited={}",
        pool_config.current_epoch,
        leaf_index,
        amount,
        burn_amount,
        deposit_amount
    );

    Ok(())
}

// ============================================================================
// Initialize Leaf Chunk for Epoch
// ============================================================================

#[derive(Accounts)]
#[instruction(epoch: u64, chunk_index: u32)]
pub struct InitializeEpochLeafChunk<'info> {
    #[account(
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(
        init,
        payer = payer,
        space = 8 + EpochLeafChunk::LEN,
        seeds = [
            b"leaves",
            pool_config.key().as_ref(),
            &epoch.to_le_bytes(),
            &chunk_index.to_le_bytes()
        ],
        bump
    )]
    pub leaf_chunk: AccountLoader<'info, EpochLeafChunk>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler_init_leaf_chunk(
    ctx: Context<InitializeEpochLeafChunk>,
    epoch: u64,
    chunk_index: u32,
) -> Result<()> {
    let mut chunk = ctx.accounts.leaf_chunk.load_init()?;
    chunk.pool = ctx.accounts.pool_config.key().to_bytes();
    chunk.epoch = epoch;
    chunk.chunk_index = chunk_index;
    chunk.count = 0;
    chunk.leaves = [[0; 32]; LEAF_CHUNK_SIZE];

    msg!("Initialized leaf chunk {} for epoch {}", chunk_index, epoch);

    Ok(())
}
