Diagnostic Poseidon Wrapper

Context

- The Merkle hashing currently uses Solana's Poseidon syscall with BN254 X5 parameters.
- Inputs are reduced to the BN254 field and passed as big-endian 32-byte slices.
- We observed opaque syscall failures (reported as "Unexpected"), so we added a diagnostic wrapper to avoid panics and surface errors.

What exists now

- `hash_two()` in `src/merkle.rs` delegates to `poseidon_hash2()`.
- `poseidon_hash2()`:
  - Reduces inputs modulo BN254.
  - Calls `solana_poseidon::hashv(Parameters::Bn254X5, Endianness::BigEndian, &[&l, &r])`.
  - Logs the exact syscall error string via `msg!` and returns `ProgramError::InvalidArgument` on failure.
  - Returns `[u8; 32]` on success.

Temporary behavior

- `hash_two()` still returns `[u8; 32]` to minimize surface changes.
- On error, it logs and returns a zero `[0u8;32]` sentinel so callers can detect failure without panics.

Refactor guidance (production)

- Change public hashing APIs to return `Result<[u8; 32], MerkleError>`.
- Replace `ProgramError::InvalidArgument` with a domain error type (e.g., `MerkleError::PoseidonFailed`).
- Remove logging from the wrapper; log only at instruction boundaries.
- Keep all Endianness/parameter choices centralized in one adapter module.
- Consider feature-gated local fallback (e.g., Keccak/Pedersen) ONLY for development flows, never for production proofs.

Verification checklist

- Inputs are big-endian, fixed 32-bytes, reduced to BN254.
- No panics are possible from hashing paths.
- Callers treat zero sentinel as an error until API returns `Result`.

Owner

- This is a temporary stop-gap to unblock local testing. Update when Poseidon inputs and padding rules are fully specified for the application.
