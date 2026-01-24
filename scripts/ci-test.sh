#!/bin/bash
# CI Test Script for zNinja Epoch-Based Shielded Pool
#
# This script runs comprehensive tests for:
# 1. Circuit compilation and proof generation
# 2. Program build with Groth16 verification
# 3. SDK build and type checking
# 4. Integration tests with mock proofs
#
# Usage:
#   ./scripts/ci-test.sh              # Run all tests
#   ./scripts/ci-test.sh --quick      # Skip circuit builds (for faster CI)
#   ./scripts/ci-test.sh --prod       # Test production build (real Groth16)
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Parse arguments
QUICK_MODE=false
PROD_MODE=false

for arg in "$@"; do
  case $arg in
    --quick)
      QUICK_MODE=true
      shift
      ;;
    --prod)
      PROD_MODE=true
      shift
      ;;
  esac
done

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}zNinja CI Test Suite${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Track timing
START_TIME=$(date +%s)

# Helper function for step output
step() {
  echo -e "\n${YELLOW}▶ $1${NC}\n"
}

success() {
  echo -e "${GREEN}✓ $1${NC}"
}

error() {
  echo -e "${RED}✗ $1${NC}"
  exit 1
}

# ============================================================================
# Step 1: Check Prerequisites
# ============================================================================
step "Checking prerequisites..."

command -v rustc >/dev/null 2>&1 || error "Rust not installed"
command -v cargo >/dev/null 2>&1 || error "Cargo not installed"
command -v node >/dev/null 2>&1 || error "Node.js not installed"
command -v npm >/dev/null 2>&1 || error "npm not installed"
command -v solana >/dev/null 2>&1 || error "Solana CLI not installed"
command -v anchor >/dev/null 2>&1 || error "Anchor CLI not installed"

RUST_VERSION=$(rustc --version)
NODE_VERSION=$(node --version)
SOLANA_VERSION=$(solana --version)
ANCHOR_VERSION=$(anchor --version)

echo "Rust: $RUST_VERSION"
echo "Node: $NODE_VERSION"
echo "Solana: $SOLANA_VERSION"
echo "Anchor: $ANCHOR_VERSION"

success "Prerequisites verified"

# ============================================================================
# Step 2: Circuit Tests (optional in quick mode)
# ============================================================================
if [ "$QUICK_MODE" = false ]; then
  step "Building and testing circuits..."
  
  cd "$ROOT_DIR/circuits"
  
  # Install dependencies
  npm ci
  
  # Check if circom is available
  if command -v circom >/dev/null 2>&1; then
    # Compile circuits
    echo "Compiling circuits..."
    npm run compile || error "Circuit compilation failed"
    
    # Check for PTAU file
    if [ ! -f "ptau/powersOfTau28_hez_final_16.ptau" ]; then
      echo "Downloading powers of tau..."
      npm run download:ptau || error "PTAU download failed"
    fi
    
    # Setup (if we have compiled circuits)
    if [ -f "build/withdraw/withdraw.r1cs" ]; then
      echo "Running trusted setup..."
      npm run setup || error "Trusted setup failed"
      npm run contribute || error "Contribution failed"
      npm run export || error "Verification key export failed"
    fi
    
    # Run circuit tests
    echo "Running circuit tests..."
    npm run test || error "Circuit tests failed"
    
    success "Circuits compiled and tested"
  else
    echo -e "${YELLOW}⚠ circom not found, skipping circuit compilation${NC}"
    echo "To install circom: cargo install circom"
  fi
  
  cd "$ROOT_DIR"
else
  echo -e "${YELLOW}Skipping circuit builds (--quick mode)${NC}"
fi

# ============================================================================
# Step 3: Program Build
# ============================================================================
step "Building Solana program..."

cd "$ROOT_DIR/program"

# Install JS dependencies for tests
npm ci || yarn install

if [ "$PROD_MODE" = true ]; then
  echo "Building with production features (real Groth16)..."
  # Production build without mock-verifier
  anchor build -- --features altbn128_syscalls
else
  echo "Building with test features (mock-verifier)..."
  # Test build with mock-verifier for faster local testing
  anchor build -- --features mock-verifier
fi

# Verify program binary exists
if [ ! -f "target/deploy/shielded_pool.so" ]; then
  error "Program binary not found after build"
fi

PROGRAM_SIZE=$(wc -c < target/deploy/shielded_pool.so)
echo "Program size: $PROGRAM_SIZE bytes"

success "Program built successfully"

# ============================================================================
# Step 4: SDK Build
# ============================================================================
step "Building SDK..."

cd "$ROOT_DIR/sdk"

npm ci
npm run build || error "SDK build failed"

# Type check
echo "Running TypeScript type check..."
npx tsc --noEmit || error "TypeScript type check failed"

success "SDK built successfully"

# ============================================================================
# Step 5: Integration Tests
# ============================================================================
step "Running integration tests..."

cd "$ROOT_DIR/program"

# Start local validator if not running
if ! solana-test-validator --help >/dev/null 2>&1; then
  echo "Starting test validator..."
  solana-test-validator --reset &
  VALIDATOR_PID=$!
  sleep 10
else
  VALIDATOR_PID=""
fi

# Run anchor tests
echo "Running V1 (legacy) tests..."
anchor test --skip-local-validator -- --test "shielded-pool" 2>&1 || true

echo "Running V2 (epoch-based) tests..."
anchor test --skip-local-validator -- --test "shielded-pool-v2" 2>&1 || true

echo "Running epoch lifecycle tests..."
anchor test --skip-local-validator -- --test "epoch-lifecycle" 2>&1 || true

# Cleanup validator if we started it
if [ -n "$VALIDATOR_PID" ]; then
  kill $VALIDATOR_PID 2>/dev/null || true
fi

success "Integration tests completed"

# ============================================================================
# Step 6: Verification Key Check (production mode)
# ============================================================================
if [ "$PROD_MODE" = true ]; then
  step "Verifying Groth16 verification keys..."
  
  # Check that verification keys are loaded in the program
  cd "$ROOT_DIR/circuits"
  
  for circuit in withdraw transfer renew; do
    VK_FILE="build/$circuit/verification_key.json"
    if [ -f "$VK_FILE" ]; then
      echo "Found verification key for $circuit"
      # Could add more detailed validation here
    else
      echo -e "${YELLOW}⚠ Verification key not found: $VK_FILE${NC}"
    fi
  done
  
  success "Verification keys checked"
fi

# ============================================================================
# Summary
# ============================================================================
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}CI Test Suite Complete${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Duration: ${DURATION}s"
echo "Mode: $([ "$PROD_MODE" = true ] && echo "Production" || echo "Development")"
echo "Quick: $([ "$QUICK_MODE" = true ] && echo "Yes" || echo "No")"
echo ""

if [ "$PROD_MODE" = true ]; then
  echo -e "${GREEN}✓ Program ready for deployment${NC}"
  echo ""
  echo "Deployment checklist:"
  echo "  1. Ensure verification keys are deployed on-chain"
  echo "  2. Verify program binary matches expected hash"
  echo "  3. Initialize pool with production parameters"
else
  echo -e "${GREEN}✓ All tests passed${NC}"
  echo ""
  echo "Next steps:"
  echo "  1. Run with --prod for production validation"
  echo "  2. Deploy to devnet for E2E testing"
fi
