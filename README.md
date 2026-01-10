## BN254/Poseidon Setup

For enabling feature-gated BN254 pairing and Poseidon syscall testing on a local validator, see `POSEIDON_SETUP.md` in this folder.

Quick builds:

```
anchor build
anchor build -- --features altbn128_syscalls
```

# Shielded Pool Anchor Program

Privacy-enhanced token transfers on Solana using ZK proofs and a frontier-only Merkle tree.

## Structure

```
program/
├── programs/shielded-pool/
│   └── src/
│       ├── lib.rs              # Program entry and instruction dispatch
│       ├── state.rs            # Account structures (PoolConfig, PoolTree, NullifierChunk)
│       ├── events.rs           # Anchor events for indexer sync
│       ├── errors.rs           # Custom error codes
│       ├── merkle.rs           # Frontier-only Merkle tree with O(depth) insertion
│       ├── nullifier.rs        # Chunked nullifier storage and lookup
│       ├── verifier.rs         # BN254 Groth16 proof verification (placeholder)
│       └── instructions/       # Instruction handlers
│           ├── initialize_pool.rs
│           ├── initialize_nullifier_chunk.rs
│           ├── initialize_verifier.rs
│           ├── deposit_shielded.rs
│           ├── shielded_transfer.rs
│           └── withdraw_shielded.rs
└── tests/                      # Anchor integration tests
```

## Build & Test

```bash
# Install dependencies
yarn install

# Build program
anchor build

# Run tests (requires localnet or test validator)
anchor test
```

## Implementation Status

**Completed:**

- ✅ Anchor scaffolding with instruction handlers
- ✅ Account structures matching spec (PoolConfig, PoolTree, NullifierChunk, VerifierConfig)
- ✅ Event schemas for indexer sync
- ✅ Token-2022 CPI integration
- ✅ Merkle tree insertion logic (frontier-only, O(depth))
- ✅ Zero-hash precomputation (Keccak256 placeholder for Poseidon)
- ✅ Nullifier storage and lookup (chunked PDAs)
- ✅ Public input decoding and validation
- ✅ Root history checking
- ✅ Full instruction logic with proof verification
- ✅ BN254 Groth16 verifier integration (placeholder for testing)
- ✅ Proof structure validation and public input checking
- ✅ Comprehensive unit tests (merkle, nullifier, verifier modules - 21 tests)
- ✅ Integration test structure

**TODO:**

- [ ] Real pairing verification (replace placeholder with Solana syscalls or ark-bn254)
- [ ] Replace Keccak256 with Poseidon hash for production
- [ ] Domain parameter computation (pool_id, chain_id from mint/config)
- [ ] Complete integration tests with localnet/devnet
- [ ] Compute unit optimization
- [ ] Security audit

## Specs Reference

See `docs/spec/` for canonical constants, PDA layouts, event schemas, and verifier integration.

## Program ID

Localnet/Devnet: `Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS`

## Features

### Merkle Tree (merkle.rs)

- **Frontier-only storage**: Stores only O(depth) nodes instead of O(2^depth)
- **O(depth) insertion**: Efficient leaf insertion with automatic parent hash propagation
- **Zero-hash precomputation**: Pre-computed hashes for empty subtrees
- **Root ring buffer**: Maintains 64 historical roots for proof validation
- **Test coverage**: 5 unit tests verifying insertion, wraparound, and capacity limits

### Nullifier Storage (nullifier.rs)

- **Chunked PDAs**: 256 nullifiers per chunk to avoid account size limits
- **Insert-only**: Prevents double-spends by enforcing uniqueness
- **Chunk indexing**: Deterministic chunk derivation from nullifier hash
- **Test coverage**: 6 unit tests for insertion, deduplication, and chunk overflow

### Proof Verifier (verifier.rs)

- **Groth16 on BN254**: Standard zkSNARK system used by Zcash, Tornado Cash, etc.
- **Proof parsing**: Correctly unpacks 256-byte proofs (pi_a, pi_b, pi_c)
- **Public input validation**: Checks all inputs are in BN254 scalar field
- **Placeholder verification**: Currently accepts valid proof structures for testing
- **Production ready interface**: Drop-in replacement for real pairing check
- **Test coverage**: 9 unit tests for proof structure, field validation, input checking

⚠️ **Note**: The verifier currently uses placeholder verification (no pairing check) to allow testing without circuits. See `docs/verifier-integration.md` for production upgrade paths.

### Instructions

1. **initialize_pool**: Creates PoolConfig and PoolTree with zero-hashes
2. **initialize_nullifier_chunk**: Creates NullifierChunk PDA for a pool
3. **initialize_verifier**: Stores Groth16 verifying key (alpha, beta, gamma, delta)
4. **deposit_shielded**: Transfers tokens to vault, inserts commitment to tree
5. **shielded_transfer**: Verifies proof, marks nullifiers spent, appends output commitments
6. **withdraw_shielded**: Verifies proof, marks nullifiers spent, transfers tokens out

## Next Steps

1. **Build circom circuits** matching the public input format (joinsplit, withdraw)
2. **Replace placeholder verifier** with real pairing check (Solana syscalls recommended)
3. **Replace Keccak256 with Poseidon** in merkle module
4. **Generate and store verifying keys** from compiled circuits
5. **Replace Keccak256 with Poseidon** for production-ready cryptography
6. **Add domain parameter helpers** to compute pool_id and chain_id
7. **Complete integration tests** with mock proofs on localnet
8. **Build circom circuits** matching the public input format
9. **Deploy to devnet** and validate against Token-2022 mint `8AnBxM3s9VUSvGtUigaP5WhLBuGtW4wnKw6wMzjREi4k`
