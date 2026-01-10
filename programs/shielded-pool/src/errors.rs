use anchor_lang::prelude::*;

#[error_code]
pub enum ShieldedPoolError {
    #[msg("Merkle tree is full")]
    TreeFull,
    #[msg("Invalid root")]
    InvalidRoot,
    #[msg("Nullifier already spent")]
    NullifierAlreadySpent,
    #[msg("Invalid proof")]
    InvalidProof,
    #[msg("Invalid public inputs")]
    InvalidPublicInputs,
    #[msg("Nullifier chunk full")]
    NullifierChunkFull,
    #[msg("Invalid commitment")]
    InvalidCommitment,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Invalid verifier config")]
    InvalidVerifierConfig,
}
