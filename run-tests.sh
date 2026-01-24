#!/bin/bash
set -e

# Run shielded pool tests
cd /Users/mintmax/Sites/zNinjaPro/program

echo "=== Configuring Solana CLI ==="
solana config set --url localhost

echo "=== Checking balance ==="
solana balance || echo "No balance yet"

echo "=== Airdropping SOL ==="
solana airdrop 100 || echo "Airdrop failed (may be ok if already has funds)"

echo "=== Building with mock-verifier feature ==="
anchor build -- --features mock-verifier

echo "=== Deploying program ==="
anchor deploy

echo "=== Running V1 Tests ==="
yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/shielded-pool.ts

echo "=== Running V2 Simple Tests ==="
yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/shielded-pool-v2-simple.ts

echo "=== All tests completed ==="
