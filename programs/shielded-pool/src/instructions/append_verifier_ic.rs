use anchor_lang::prelude::*;
use crate::errors::ShieldedPoolError;
use crate::state::{PoolConfig, VerifierConfig, CircuitType, MAX_VERIFIER_IC_POINTS};

#[derive(Accounts)]
#[instruction(circuit: CircuitType)]
pub struct AppendVerifierIc<'info> {
    #[account(
        mut,
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

    pub authority: Signer<'info>,
}

pub fn handler(
    ctx: Context<AppendVerifierIc>,
    circuit: CircuitType,
    ic_points: Vec<[[u8; 32]; 2]>,
) -> Result<()> {
    let mut verifier = ctx.accounts.verifier_config.load_mut()?;
    require!(verifier.circuit == circuit.as_u8(), ShieldedPoolError::InvalidVerifierConfig);
    let start = verifier.vk_len_ic as usize;
    let new_len = start + ic_points.len();

    require!(new_len <= MAX_VERIFIER_IC_POINTS, ShieldedPoolError::InvalidVerifierConfig);

    for (i, src) in ic_points.iter().enumerate() {
        let dst_index = start + i;
        let mut flattened = [0u8; 64];
        flattened[..32].copy_from_slice(&src[0]);
        flattened[32..].copy_from_slice(&src[1]);
        verifier.ic[dst_index] = flattened;
    }

    verifier.vk_len_ic = new_len as u16;
    Ok(())
}
