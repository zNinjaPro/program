use anchor_lang::prelude::*;
use crate::state::*;

#[derive(Accounts)]
#[instruction(pool_id: [u8; 32], chunk_index: u32)]
pub struct InitializeNullifierChunk<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + NullifierChunk::LEN,
        seeds = [b"nullifier", pool_id.as_ref(), &chunk_index.to_be_bytes()],
        bump
    )]
    pub nullifier_chunk: AccountLoader<'info, NullifierChunk>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeNullifierChunk>,
    pool_id: [u8; 32],
    chunk_index: u32,
) -> Result<()> {
    let mut chunk = ctx.accounts.nullifier_chunk.load_init()?;
    chunk.pool_id = pool_id;
    chunk.chunk_index = chunk_index;
    chunk.count = 0;
    // Zero-copy nodes are already zeroed on init
    chunk.nodes = [[0; 32]; MAX_NULLIFIER_CHUNK_SIZE];

    Ok(())
}
