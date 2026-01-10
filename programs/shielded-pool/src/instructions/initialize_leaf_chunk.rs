use anchor_lang::prelude::*;
use crate::state::*;

#[derive(Accounts)]
#[instruction(chunk_index: u32)]
pub struct InitializeLeafChunk<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + LeafChunk::LEN,
        seeds = [b"leaf", mint.key().as_ref(), &chunk_index.to_be_bytes()],
        bump
    )]
    pub leaf_chunk: AccountLoader<'info, LeafChunk>,

    /// CHECK: Token-2022 mint pubkey used in PDA seeds
    pub mint: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeLeafChunk>,
    chunk_index: u32,
) -> Result<()> {
    let mut chunk = ctx.accounts.leaf_chunk.load_init()?;
    chunk.mint = ctx.accounts.mint.key();
    chunk.chunk_index = chunk_index;
    chunk.count = 0;
    // Zero-copy leaves are already zeroed on init
    chunk.leaves = [[0u8; 32]; LEAF_CHUNK_SIZE];
    Ok(())
}
