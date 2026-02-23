use anchor_lang::prelude::*;

#[error_code]
pub enum ShieldedPoolError {
    // ========== General Errors ==========
    #[msg("Math overflow")]
    MathOverflow,

    #[msg("Invalid proof")]
    InvalidProof,

    #[msg("Invalid public inputs")]
    InvalidPublicInputs,

    #[msg("Invalid verifier config")]
    InvalidVerifierConfig,

    #[msg("Invalid commitment")]
    InvalidCommitment,

    #[msg("Invalid amount")]
    InvalidAmount,

    // ========== Pool State Errors ==========
    #[msg("Pool is paused")]
    PoolPaused,

    #[msg("Unauthorized - not pool authority")]
    Unauthorized,

    // ========== Epoch Errors ==========
    #[msg("Epoch is not active, cannot deposit")]
    EpochNotActive,

    #[msg("Epoch is not frozen, cannot finalize")]
    EpochNotFrozen,

    #[msg("Epoch is not finalized, cannot spend")]
    EpochNotFinalized,

    #[msg("Epoch has expired, cannot spend")]
    EpochExpired,

    #[msg("Epoch has not expired yet, cannot garbage collect")]
    EpochNotExpired,

    #[msg("Epoch is not ready for rollover")]
    EpochNotReadyForRollover,

    #[msg("Epoch is not ready for finalization")]
    EpochNotReadyForFinalization,

    #[msg("Invalid epoch number")]
    InvalidEpoch,

    // ========== Merkle Tree Errors ==========
    #[msg("Merkle tree is full")]
    TreeFull,

    #[msg("Invalid Merkle root")]
    InvalidRoot,

    #[msg("Leaf chunk is full")]
    LeafChunkFull,

    #[msg("Poseidon hash computation failed")]
    PoseidonHashError,

    // ========== Nullifier Errors ==========
    #[msg("Nullifier already exists (double-spend attempt)")]
    NullifierAlreadyExists,

    #[msg("Nullifier chunk full")]
    NullifierChunkFull,

    // ========== Legacy Errors (for compatibility) ==========
    #[msg("Nullifier already spent")]
    NullifierAlreadySpent,

    // ========== Token Gate & Burn Errors ==========
    #[msg("User must hold the pool token to transact")]
    InsufficientGateBalance,

    #[msg("Burn rate exceeds maximum allowed (10%)")]
    InvalidBurnRate,

    #[msg("Burn calculation overflow")]
    BurnOverflow,

    // ========== Security Hardening (Task 8) ==========
    #[msg("Chain ID mismatch — possible cross-chain replay")]
    InvalidChainId,

    #[msg("Recipient does not match proof public input")]
    InvalidRecipient,

    #[msg("Transfer outputs span two leaf chunks — retry when aligned")]
    LeafChunkBoundary,

    #[msg("Account is not a valid NullifierMarker")]
    InvalidNullifierMarker,

    // ========== Task 9 — MEDIUM Security Hardening ==========
    #[msg("Vault has insufficient balance for withdrawal")]
    InsufficientVaultBalance,
}
