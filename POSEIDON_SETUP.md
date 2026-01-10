# Poseidon + Alt_bn128 Syscalls Setup

This guide shows how to build, deploy, and test the program with feature-gated BN254 pairing and Poseidon hashing on a local validator.

## Prerequisites

- Solana/Agave toolchain installed and on `PATH` (use your custom `agave` build if testing syscalls).
- Anchor CLI installed (`anchor --version` prints). v0.32.x recommended for Solana 2.x.
- Rust toolchain aligned with project (e.g., `rustc 1.79+`).

## Cargo Feature: `altbn128_syscalls`

- The program wires BN254 pairing via feature-gated FFI.
- When the feature is disabled, the program logs a fallback message and treats the pairing as valid for dev-only testing.
- When enabled AND running on a validator that exposes the `alt_bn128_pairing` syscall, the verifier uses the syscall.

### Enable/Disable

- Enable for build:
  ```sh
  cd program
  cargo build --release -p shielded-pool --features altbn128_syscalls
  ```
- With Anchor (all programs):
  ```sh
  cd program
  anchor build -- --features altbn128_syscalls
  ```
- Disable (default): build without `--features` flag.

## Local Validator Modes

- Fallback mode (default): Use stock validator or custom validator without BN254 features.
  - Expected logs on `verify_pairing` instruction:
    - `alt_bn128_pairing FALLBACK: treating N pairings as valid`
    - `verify_pairing: pairing result = true`
- Syscall mode: Use custom Agave validator exposing BN254 pairing + Poseidon syscalls.
  - Ensure your custom binaries are first on `PATH`:
    ```sh
    export PATH="$HOME/src/agave/bin:$PATH"
    solana-test-validator --version
    ```
  - Start fresh:
    ```sh
    pkill -9 solana-test-validator 2>/dev/null || true
    solana-test-validator --reset --quiet --log /tmp/custom_validator.log &
    tail -50 /tmp/custom_validator.log
    ```

## Build + Deploy

```sh
# Build everything (fallback)
cd program
anchor build

# Build using BN254 syscalls (if validator supports them)
anchor build -- --features altbn128_syscalls

# Deploy (ensure validator is running)
anchor deploy
```

## Fast Tests

- Pairing-only test:
  ```sh
  cd sdk
  npx ts-node pairing-only-test.ts
  # Then confirm logs
  solana confirm -v <SIG> | grep -E "Program log|verify_pairing|alt_bn128"
  ```
- Poseidon-only test (deposit path):
  ```sh
  cd sdk
  npx ts-node poseidon-only-test.ts
  ```

## Troubleshooting

- "unresolved symbol" at link time: build without `altbn128_syscalls` or run against custom validator exposing the syscall.
- Token program mismatches: use Token-2022 (`TOKEN_2022_PROGRAM_ID`) for deposit paths and ensure vault/user ATAs are created.
- Slow E2E tests: prefer the minimal tests above; pre-create mints/ATAs and run against local RPC.

## Notes

- Feature gating ensures production deployments don’t depend on experimental syscalls.
- When enabling syscalls, confirm validator feature status and program logs to verify path selection.
