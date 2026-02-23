// ============================================================================
// Legacy Instructions (v1) — gated behind `legacy-v1` feature
// ============================================================================
#[cfg(feature = "legacy-v1")]
pub mod initialize_pool;
#[cfg(feature = "legacy-v1")]
pub mod initialize_nullifier_chunk;
#[cfg(feature = "legacy-v1")]
pub mod initialize_leaf_chunk;
#[cfg(feature = "legacy-v1")]
pub mod deposit_shielded;
#[cfg(feature = "legacy-v1")]
pub mod shielded_transfer;
#[cfg(feature = "legacy-v1")]
pub mod withdraw_shielded;

#[cfg(feature = "legacy-v1")]
pub use initialize_pool::*;
#[cfg(feature = "legacy-v1")]
pub use initialize_nullifier_chunk::*;
#[cfg(feature = "legacy-v1")]
pub use initialize_leaf_chunk::*;
#[cfg(feature = "legacy-v1")]
pub use deposit_shielded::*;
#[cfg(feature = "legacy-v1")]
pub use shielded_transfer::*;
#[cfg(feature = "legacy-v1")]
pub use withdraw_shielded::*;

// Verifier setup instructions — shared between V1 and V2
pub mod initialize_verifier;
pub mod append_verifier_ic;
pub use initialize_verifier::*;
pub use append_verifier_ic::*;

// ============================================================================
// Epoch-Based Instructions (v2)
// ============================================================================
pub mod initialize_pool_v2;
pub mod rollover_epoch;
pub mod finalize_epoch;
pub mod deposit_v2;
pub mod withdraw_v2;
pub mod transfer_v2;
pub mod renew_note;
pub mod garbage_collect;
pub mod admin;

pub use initialize_pool_v2::*;
pub use rollover_epoch::*;
pub use finalize_epoch::*;
pub use deposit_v2::*;
pub use withdraw_v2::*;
pub use transfer_v2::*;
pub use renew_note::*;
pub use garbage_collect::*;
pub use admin::*;
