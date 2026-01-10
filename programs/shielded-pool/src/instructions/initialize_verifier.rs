use anchor_lang::prelude::*;
use crate::errors::ShieldedPoolError;
use crate::state::{CircuitType, PoolConfig, VerifierConfig, MAX_VERIFIER_IC_POINTS};

#[derive(Accounts)]
#[instruction(circuit: CircuitType)]
pub struct InitializeVerifier<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + VerifierConfig::LEN,
        seeds = [
            b"verifier",
            pool_config.key().as_ref(),
            circuit.seed(),
        ],
        bump
    )]
    pub verifier_config: AccountLoader<'info, VerifierConfig>,

    #[account(
        seeds = [b"config", pool_config.mint.as_ref()],
        bump = pool_config.program_bump,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeVerifier>,
    circuit: CircuitType,
    vk_alpha: [[u8; 32]; 2],
    vk_beta: [[u8; 32]; 4],
    vk_gamma: [[u8; 32]; 4],
    vk_delta: [[u8; 32]; 4],
    ic_points: Vec<[[u8; 32]; 2]>,
) -> Result<()> {
    msg!(
        "initialize_verifier: payer={}, signer={}, pool_config={} verifier_config={}",
        ctx.accounts.payer.key(),
        ctx.accounts.payer.is_signer,
        ctx.accounts.pool_config.key(),
        ctx.accounts.verifier_config.key(),
    );

    let mut verifier = ctx.accounts.verifier_config.load_init()?;
    require!(
        ic_points.len() <= MAX_VERIFIER_IC_POINTS,
        ShieldedPoolError::InvalidVerifierConfig
    );

    verifier.version = 1;
    verifier.circuit = circuit.as_u8();
    verifier._padding0 = 0;
    verifier.vk_len_ic = ic_points.len() as u16;
    verifier._padding1 = 0;
    verifier.vk_galpha = vk_alpha;
    verifier.vk_hbeta = vk_beta;
    verifier.vk_ggamma = vk_gamma;
    verifier.vk_hdelta = vk_delta;
    verifier.ic.iter_mut().for_each(|point| *point = [0u8; 64]);

    for (dst, src) in verifier.ic.iter_mut().zip(ic_points.iter()) {
        let mut flattened = [0u8; 64];
        flattened[..32].copy_from_slice(&src[0]);
        flattened[32..].copy_from_slice(&src[1]);
        *dst = flattened;
    }

    Ok(())
}
