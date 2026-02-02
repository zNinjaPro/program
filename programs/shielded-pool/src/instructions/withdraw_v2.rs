use anchor_lang::prelude::*;
use anchor_spl::token_interface::{burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked};
use crate::state::*;
use crate::errors::ShieldedPoolError;
use crate::events::WithdrawEvent;
use crate::verifier::{Groth16Proof, VerifyingKey, verify_groth16_proof};

/// Public inputs for withdraw circuit
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct WithdrawPublicInputs {
    pub root: [u8; 32],
    pub nullifier: [u8; 32],
    pub amount: u64,
    pub recipient: Pubkey,
    pub epoch: u64,
    pub tx_anchor: [u8; 32],
    pub pool_id: [u8; 32],
    pub chain_id: [u8; 32],
}

#[derive(Accounts)]
#[instruction(proof_bytes: Vec<u8>, public_inputs: WithdrawPublicInputs)]
pub struct WithdrawV2<'info> {
    #[account(
        mut,
        seeds = [b"pool_config", pool_config.mint.as_ref()],
        bump = pool_config.config_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// Epoch tree being spent from (must be Finalized)
    #[account(
        seeds = [b"epoch_tree", pool_config.key().as_ref(), &public_inputs.epoch.to_le_bytes()],
        bump = epoch_tree.load()?.bump,
    )]
    pub epoch_tree: AccountLoader<'info, EpochTree>,

    /// Nullifier marker PDA - will be created to mark nullifier as spent
    #[account(
        init,
        payer = payer,
        space = NullifierMarker::LEN,
        seeds = [
            b"nullifier",
            pool_config.key().as_ref(),
            &public_inputs.epoch.to_le_bytes(),
            &public_inputs.nullifier
        ],
        bump
    )]
    pub nullifier_marker: Account<'info, NullifierMarker>,

    /// Verifier config for withdraw circuit
    #[account(
        seeds = [b"verifier", pool_config.key().as_ref(), CircuitType::Withdraw.seed()],
        bump,
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    /// CHECK: Vault authority PDA
    #[account(
        seeds = [b"vault_authority", pool_config.key().as_ref()],
        bump = pool_config.vault_authority_bump,
    )]
    pub vault_authority: UncheckedAccount<'info>,

    /// Token vault
    #[account(
        mut,
        address = pool_config.vault,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    /// Recipient's token account - also serves as gate validation (must have non-zero balance after receive)
    #[account(
        mut,
        constraint = recipient_token_account.mint == pool_config.mint @ ShieldedPoolError::InvalidCommitment,
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    /// Token mint
    #[account(
        address = pool_config.mint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<WithdrawV2>,
    proof_bytes: Vec<u8>,
    public_inputs: WithdrawPublicInputs,
) -> Result<()> {
    let pool_config = &mut ctx.accounts.pool_config;
    let clock = Clock::get()?;

    // Validate pool is not paused
    require!(!pool_config.paused, ShieldedPoolError::PoolPaused);

    // Validate epoch is finalized and not expired
    let tree = ctx.accounts.epoch_tree.load()?;
    require!(
        tree.get_state() == EpochState::Finalized,
        ShieldedPoolError::EpochNotFinalized
    );

    let expiry_slot = tree.finalized_slot
        .checked_add(pool_config.expiry_slots)
        .ok_or(ShieldedPoolError::MathOverflow)?;
    
    require!(
        clock.slot < expiry_slot,
        ShieldedPoolError::EpochExpired
    );

    // Validate root matches finalized root
    require!(
        public_inputs.root == tree.final_root,
        ShieldedPoolError::InvalidRoot
    );

    // Validate pool_id matches
    require!(
        public_inputs.pool_id == pool_config.key().to_bytes(),
        ShieldedPoolError::InvalidPublicInputs
    );

    // Verify Groth16 proof
    let proof = Box::new(Groth16Proof::from_bytes(&proof_bytes)?);
    let verifier_account = ctx.accounts.verifier_config.load()?;
    let vk = Box::new(VerifyingKey::from(&*verifier_account));
    let public_inputs_array = public_inputs_to_field_elements(&public_inputs);
    let is_valid = verify_groth16_proof(&proof, &public_inputs_array, &vk)?;
    require!(is_valid, ShieldedPoolError::InvalidProof);

    // Initialize nullifier marker (existence prevents double-spend)
    let nullifier_marker = &mut ctx.accounts.nullifier_marker;
    nullifier_marker.pool = pool_config.key();
    nullifier_marker.epoch = public_inputs.epoch;
    nullifier_marker.nullifier = public_inputs.nullifier;
    nullifier_marker.bump = ctx.bumps.nullifier_marker;

    // Calculate burn amount (0.1% = 10 basis points by default)
    let burn_amount = public_inputs.amount
        .checked_mul(pool_config.burn_rate_bps as u64)
        .ok_or(ShieldedPoolError::BurnOverflow)?
        .checked_div(10_000)
        .ok_or(ShieldedPoolError::BurnOverflow)?;
    
    let withdraw_amount = public_inputs.amount
        .checked_sub(burn_amount)
        .ok_or(ShieldedPoolError::BurnOverflow)?;

    // Transfer tokens from vault to recipient
    let pool_key = pool_config.key();
    let seeds = &[
        b"vault_authority".as_ref(),
        pool_key.as_ref(),
        &[pool_config.vault_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    // Burn the burn_amount from vault
    if burn_amount > 0 {
        let burn_cpi_accounts = Burn {
            mint: ctx.accounts.mint.to_account_info(),
            from: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.vault_authority.to_account_info(),
        };
        let burn_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            burn_cpi_accounts,
            signer_seeds,
        );
        burn(burn_cpi_ctx, burn_amount)?;

        // Update cumulative burn counter
        pool_config.total_burned = pool_config.total_burned.saturating_add(burn_amount);
    }

    // Transfer remaining tokens to recipient
    let cpi_accounts = TransferChecked {
        from: ctx.accounts.vault.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.recipient_token_account.to_account_info(),
        authority: ctx.accounts.vault_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    transfer_checked(cpi_ctx, withdraw_amount, ctx.accounts.mint.decimals)?;

    // Update pool stats
    pool_config.total_withdrawals = pool_config.total_withdrawals.saturating_add(1);

    emit!(WithdrawEvent {
        pool: pool_config.key(),
        epoch: public_inputs.epoch,
        nullifier: public_inputs.nullifier,
        amount: public_inputs.amount,
        recipient: public_inputs.recipient,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Withdraw: epoch={}, amount={}, burned={}, sent={}, recipient={}",
        public_inputs.epoch,
        public_inputs.amount,
        burn_amount,
        withdraw_amount,
        public_inputs.recipient
    );

    Ok(())
}

/// Convert public inputs to field elements for proof verification
fn public_inputs_to_field_elements(inputs: &WithdrawPublicInputs) -> Vec<[u8; 32]> {
    let mut elements = Vec::with_capacity(8);
    
    elements.push(inputs.root);
    elements.push(inputs.nullifier);
    
    // Convert amount to 32-byte field element (little-endian)
    let mut amount_bytes = [0u8; 32];
    amount_bytes[..8].copy_from_slice(&inputs.amount.to_le_bytes());
    elements.push(amount_bytes);
    
    // Convert recipient to 32-byte field element
    elements.push(inputs.recipient.to_bytes());
    
    // Convert epoch to 32-byte field element
    let mut epoch_bytes = [0u8; 32];
    epoch_bytes[..8].copy_from_slice(&inputs.epoch.to_le_bytes());
    elements.push(epoch_bytes);
    
    elements.push(inputs.tx_anchor);
    elements.push(inputs.pool_id);
    elements.push(inputs.chain_id);
    
    elements
}
