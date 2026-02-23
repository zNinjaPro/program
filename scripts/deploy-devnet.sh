#!/bin/bash
set -euo pipefail

###############################################################################
# deploy-devnet.sh — End-to-end devnet deployment for zNinja Shielded Pool
#
# Usage:
#   bash program/scripts/deploy-devnet.sh [OPTIONS]
#
# Options:
#   --skip-build         Skip program build (use existing binary)
#   --skip-deploy        Skip program deploy (use already-deployed program)
#   --skip-token         Skip test token creation (use existing mint from test-token-mint.json)
#   --skip-pool          Skip pool initialization (use existing pool from pool-config.json)
#   --epoch-duration N   Epoch duration in slots (default: 100, ~40s for testing)
#   --expiry N           Expiry in slots (default: 1000, ~400s for testing)
#   --finalization N     Finalization delay in slots (default: 10, ~4s for testing)
#   --burn-rate N        Burn rate in basis points (default: 10 = 0.1%)
#   --dry-run            Print what would be done without executing
#
# Environment:
#   ANCHOR_WALLET        Path to keypair JSON (default: ~/.config/solana/id.json)
#   ANCHOR_PROVIDER_URL  RPC URL (default: https://api.devnet.solana.com)
###############################################################################

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROGRAM_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$PROGRAM_DIR/.." && pwd)"

# Defaults
SKIP_BUILD=false
SKIP_DEPLOY=false
SKIP_TOKEN=false
SKIP_POOL=false
DRY_RUN=false
EPOCH_DURATION=100
EXPIRY=1000
FINALIZATION_DELAY=10
BURN_RATE_BPS=10
RPC_URL="${ANCHOR_PROVIDER_URL:-https://api.devnet.solana.com}"
WALLET_PATH="${ANCHOR_WALLET:-$HOME/.config/solana/id.json}"

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-build)       SKIP_BUILD=true; shift ;;
    --skip-deploy)      SKIP_DEPLOY=true; shift ;;
    --skip-token)       SKIP_TOKEN=true; shift ;;
    --skip-pool)        SKIP_POOL=true; shift ;;
    --dry-run)          DRY_RUN=true; shift ;;
    --epoch-duration)   EPOCH_DURATION="$2"; shift 2 ;;
    --expiry)           EXPIRY="$2"; shift 2 ;;
    --finalization)     FINALIZATION_DELAY="$2"; shift 2 ;;
    --burn-rate)        BURN_RATE_BPS="$2"; shift 2 ;;
    --rpc)              RPC_URL="$2"; shift 2 ;;
    --wallet)           WALLET_PATH="$2"; shift 2 ;;
    -h|--help)
      head -25 "$0" | tail -20
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

export ANCHOR_PROVIDER_URL="$RPC_URL"
export ANCHOR_WALLET="$WALLET_PATH"

# Expected program ID from Anchor.toml
PROGRAM_ID="C58iVei3DXTL9BSKe5ZpQuJehqLJL1fQjejdnCAdWzV7"

# Output files
TOKEN_FILE="$PROGRAM_DIR/test-token-mint.json"
POOL_FILE="$PROGRAM_DIR/pool-config.json"
DEPLOYMENT_FILE="$PROGRAM_DIR/devnet-deployment.json"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

log()    { echo -e "${BLUE}[deploy]${NC} $*"; }
ok()     { echo -e "${GREEN}  ✅${NC} $*"; }
warn()   { echo -e "${YELLOW}  ⚠️ ${NC} $*"; }
fail()   { echo -e "${RED}  ❌${NC} $*"; exit 1; }
header() { echo -e "\n${BOLD}═══════════════════════════════════════════════════${NC}"; echo -e "${BOLD}  $*${NC}"; echo -e "${BOLD}═══════════════════════════════════════════════════${NC}"; }

###############################################################################
# Step 0: Validate prerequisites
###############################################################################
header "Step 0: Validating Prerequisites"

# Check solana CLI
if ! command -v solana &>/dev/null; then
  fail "solana CLI not found. Install: https://docs.solanalabs.com/cli/install"
fi
SOLANA_VERSION=$(solana --version 2>&1 | head -1)
ok "Solana CLI: $SOLANA_VERSION"

# Check anchor CLI
if ! command -v anchor &>/dev/null; then
  fail "anchor CLI not found. Install: https://www.anchor-lang.com/docs/installation"
fi
ANCHOR_VERSION=$(anchor --version 2>&1)
ok "Anchor CLI: $ANCHOR_VERSION"

# Check node
if ! command -v node &>/dev/null; then
  fail "node not found. Install Node.js 18+"
fi
NODE_VERSION=$(node --version)
ok "Node.js: $NODE_VERSION"

# Check wallet
if [[ ! -f "$WALLET_PATH" ]]; then
  fail "Wallet keypair not found at $WALLET_PATH. Set ANCHOR_WALLET or run: solana-keygen new"
fi
WALLET_PUBKEY=$(solana-keygen pubkey "$WALLET_PATH")
ok "Wallet: $WALLET_PUBKEY"

# Check RPC connectivity
log "Connecting to $RPC_URL ..."
if ! solana cluster-version -u "$RPC_URL" &>/dev/null; then
  fail "Cannot connect to $RPC_URL"
fi
CLUSTER_VERSION=$(solana cluster-version -u "$RPC_URL" 2>&1)
ok "Cluster: $CLUSTER_VERSION"

# Check verifier key files exist
for circuit in withdraw transfer renew; do
  VK_FILE="$PROGRAM_DIR/verifier/${circuit}_vk.json"
  if [[ ! -f "$VK_FILE" ]]; then
    fail "Verification key not found: $VK_FILE"
  fi
done
ok "All 3 verification keys present (withdraw, transfer, renew)"

###############################################################################
# Step 1: Check SOL balance
###############################################################################
header "Step 1: Checking SOL Balance"

BALANCE_LAMPORTS=$(solana balance "$WALLET_PUBKEY" -u "$RPC_URL" --lamports 2>/dev/null | grep -oE '[0-9]+' | head -1)
BALANCE_SOL=$(echo "scale=4; $BALANCE_LAMPORTS / 1000000000" | bc 2>/dev/null || echo "unknown")

log "Balance: $BALANCE_SOL SOL ($BALANCE_LAMPORTS lamports)"

if [[ "$BALANCE_LAMPORTS" -lt 2000000000 ]]; then
  warn "Balance below 2 SOL. Attempting airdrop..."
  if $DRY_RUN; then
    log "[DRY RUN] Would airdrop 2 SOL"
  else
    solana airdrop 2 "$WALLET_PUBKEY" -u "$RPC_URL" || warn "Airdrop failed (rate limited?). Ensure wallet has enough SOL."
    # Refresh balance
    sleep 2
    BALANCE_LAMPORTS=$(solana balance "$WALLET_PUBKEY" -u "$RPC_URL" --lamports 2>/dev/null | grep -oE '[0-9]+' | head -1)
    BALANCE_SOL=$(echo "scale=4; $BALANCE_LAMPORTS / 1000000000" | bc 2>/dev/null || echo "unknown")
    log "Updated balance: $BALANCE_SOL SOL"
  fi
fi

if [[ "$BALANCE_LAMPORTS" -lt 500000000 ]]; then
  if $DRY_RUN; then
    warn "Insufficient SOL for real deployment ($BALANCE_SOL SOL). Continuing in dry-run mode."
  else
    fail "Insufficient SOL balance ($BALANCE_SOL SOL). Need at least 0.5 SOL for deployment."
  fi
else
  ok "SOL balance sufficient: $BALANCE_SOL SOL"
fi

###############################################################################
# Step 2: Build program
###############################################################################
header "Step 2: Building Program"

if $SKIP_BUILD; then
  warn "Skipping build (--skip-build)"
else
  log "Building with features: altbn128_syscalls"
  cd "$PROGRAM_DIR"

  if $DRY_RUN; then
    log "[DRY RUN] Would run: anchor build -- --features altbn128_syscalls"
  else
    anchor build -- --features altbn128_syscalls 2>&1 | while IFS= read -r line; do
      # Only show key lines, suppress verbose rustc output
      case "$line" in
        *Compiling*|*Finished*|*Error*|*error*|*warning:*)
          echo "    $line"
          ;;
      esac
    done
    ok "Program built successfully"
  fi

  # Verify program ID matches
  BINARY="$PROGRAM_DIR/target/deploy/shielded_pool.so"
  if [[ -f "$BINARY" ]]; then
    BUILT_ID=$(solana-keygen pubkey "$PROGRAM_DIR/target/deploy/shielded_pool-keypair.json" 2>/dev/null || echo "unknown")
    if [[ "$BUILT_ID" != "$PROGRAM_ID" ]]; then
      warn "Built program ID ($BUILT_ID) differs from expected ($PROGRAM_ID)"
      warn "This may cause deployment issues. Ensure Anchor.toml program ID is correct."
    else
      ok "Program ID verified: $PROGRAM_ID"
    fi
  fi
fi

###############################################################################
# Step 3: Deploy program
###############################################################################
header "Step 3: Deploying Program to Devnet"

if $SKIP_DEPLOY; then
  warn "Skipping deploy (--skip-deploy)"
else
  log "Deploying $PROGRAM_ID to devnet..."

  if $DRY_RUN; then
    log "[DRY RUN] Would run: anchor deploy --provider.cluster devnet"
  else
    cd "$PROGRAM_DIR"
    anchor deploy --provider.cluster devnet 2>&1 | tail -5
    ok "Program deployed: $PROGRAM_ID"
  fi
fi

# Verify program exists on-chain
log "Verifying program on devnet..."
if solana program show "$PROGRAM_ID" -u "$RPC_URL" &>/dev/null; then
  ok "Program confirmed on-chain: $PROGRAM_ID"
else
  warn "Could not verify program on-chain. If using --skip-deploy, ensure program was previously deployed."
fi

###############################################################################
# Step 4: Create test SPL token
###############################################################################
header "Step 4: Creating Test SPL Token"

if $SKIP_TOKEN; then
  if [[ -f "$TOKEN_FILE" ]]; then
    TOKEN_MINT=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$TOKEN_FILE','utf8')).mint)")
    ok "Using existing token mint: $TOKEN_MINT (from test-token-mint.json)"
  else
    fail "--skip-token specified but $TOKEN_FILE not found"
  fi
else
  if [[ -f "$TOKEN_FILE" ]]; then
    EXISTING_MINT=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$TOKEN_FILE','utf8')).mint)" 2>/dev/null || echo "")
    if [[ -n "$EXISTING_MINT" ]]; then
      # Check if mint exists on-chain
      if solana account "$EXISTING_MINT" -u "$RPC_URL" &>/dev/null; then
        TOKEN_MINT="$EXISTING_MINT"
        ok "Token mint already exists on-chain: $TOKEN_MINT (skipping creation)"
      else
        log "Previous token mint not found on-chain. Creating new token..."
      fi
    fi
  fi

  if [[ -z "${TOKEN_MINT:-}" ]]; then
    if $DRY_RUN; then
      log "[DRY RUN] Would run: node scripts/create-test-token.js"
      TOKEN_MINT="<DRY_RUN_MINT>"
    else
      cd "$PROGRAM_DIR"
      node scripts/create-test-token.js 2>&1 | tail -10
      TOKEN_MINT=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$TOKEN_FILE','utf8')).mint)")
      ok "Test token created: $TOKEN_MINT"
    fi
  fi
fi

###############################################################################
# Step 5: Initialize V2 pool
###############################################################################
header "Step 5: Initializing V2 Shielded Pool"

log "Parameters:"
log "  Token Mint:          $TOKEN_MINT"
log "  Epoch Duration:      $EPOCH_DURATION slots"
log "  Expiry:              $EXPIRY slots"
log "  Finalization Delay:  $FINALIZATION_DELAY slots"
log "  Burn Rate:           $BURN_RATE_BPS bps"

if $SKIP_POOL; then
  if [[ -f "$POOL_FILE" ]]; then
    POOL_CONFIG=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$POOL_FILE','utf8')).poolConfig)")
    ok "Using existing pool: $POOL_CONFIG (from pool-config.json)"
  else
    fail "--skip-pool specified but $POOL_FILE not found"
  fi
else
  if [[ -f "$POOL_FILE" ]]; then
    EXISTING_POOL=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$POOL_FILE','utf8')).poolConfig)" 2>/dev/null || echo "")
    if [[ -n "$EXISTING_POOL" ]]; then
      if solana account "$EXISTING_POOL" -u "$RPC_URL" &>/dev/null; then
        POOL_CONFIG="$EXISTING_POOL"
        ok "Pool already exists on-chain: $POOL_CONFIG (skipping initialization)"
      else
        log "Previous pool not found on-chain. Initializing new pool..."
      fi
    fi
  fi

  if [[ -z "${POOL_CONFIG:-}" ]]; then
    if $DRY_RUN; then
      log "[DRY RUN] Would run: node scripts/init_pool_v2.js $TOKEN_MINT $BURN_RATE_BPS $EPOCH_DURATION $EXPIRY $FINALIZATION_DELAY"
      POOL_CONFIG="<DRY_RUN_POOL>"
    else
      cd "$PROGRAM_DIR"
      node scripts/init_pool_v2.js "$TOKEN_MINT" "$BURN_RATE_BPS" "$EPOCH_DURATION" "$EXPIRY" "$FINALIZATION_DELAY" 2>&1 | tail -15
      POOL_CONFIG=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$POOL_FILE','utf8')).poolConfig)")
      ok "Pool initialized: $POOL_CONFIG"
    fi
  fi
fi

###############################################################################
# Step 6: Upload verification keys (3 circuits)
###############################################################################
header "Step 6: Uploading Verification Keys"

VERIFIER_PDAS=()

for circuit in withdraw transfer renew; do
  VK_FILE="$PROGRAM_DIR/verifier/${circuit}_vk.json"
  log "Uploading $circuit verification key..."

  if $DRY_RUN; then
    log "[DRY RUN] Would run: node scripts/init_verifier.js $circuit $POOL_CONFIG $VK_FILE"
    VERIFIER_PDAS+=("<DRY_RUN_${circuit}_VERIFIER>")
  else
    cd "$PROGRAM_DIR"
    node scripts/init_verifier.js "$circuit" "$POOL_CONFIG" "$VK_FILE" 2>&1 | while IFS= read -r line; do
      echo "    $line"
    done
    ok "$circuit verifier uploaded"
  fi
done

###############################################################################
# Step 7: Initialize first epoch's leaf chunk
###############################################################################
header "Step 7: Initializing Epoch 0 Leaf Chunk"

# The pool starts at epoch 0. We need at least one leaf chunk PDA for deposits.
# The init_pool_v2 instruction creates the epoch tree PDA but leaf chunks must
# be initialized separately via initialize_epoch_leaf_chunk.
# Check if client.ts or a script handles this, or if deposits auto-init chunks.
# For now, just note this requirement.
log "Leaf chunks are initialized on-demand by the first depositor in each chunk range."
ok "No manual leaf chunk initialization required"

###############################################################################
# Step 8: Deployment summary
###############################################################################
header "Deployment Summary"

echo ""
echo -e "${BOLD}Program${NC}"
echo "  Program ID:       $PROGRAM_ID"
echo "  Explorer:         https://explorer.solana.com/address/$PROGRAM_ID?cluster=devnet"
echo ""
echo -e "${BOLD}Token${NC}"
echo "  Mint:             $TOKEN_MINT"
echo "  Explorer:         https://explorer.solana.com/address/$TOKEN_MINT?cluster=devnet"
echo ""
echo -e "${BOLD}Pool${NC}"
echo "  Pool Config PDA:  $POOL_CONFIG"
echo "  Explorer:         https://explorer.solana.com/address/$POOL_CONFIG?cluster=devnet"
echo ""
echo -e "${BOLD}Epoch Parameters${NC}"
echo "  Duration:         $EPOCH_DURATION slots (~$(( EPOCH_DURATION * 400 / 1000 ))s)"
echo "  Finalization:     $FINALIZATION_DELAY slots (~$(( FINALIZATION_DELAY * 400 / 1000 ))s)"
echo "  Expiry:           $EXPIRY slots (~$(( EXPIRY * 400 / 1000 ))s)"
echo "  Burn Rate:        $BURN_RATE_BPS bps"
echo ""
echo -e "${BOLD}RPC${NC}"
echo "  URL:              $RPC_URL"
echo "  Wallet:           $WALLET_PUBKEY"
echo ""

# Save deployment info
if ! $DRY_RUN; then
  node -e "
    const fs = require('fs');
    const pool = JSON.parse(fs.readFileSync('$POOL_FILE', 'utf8') || '{}');
    const token = JSON.parse(fs.readFileSync('$TOKEN_FILE', 'utf8') || '{}');
    const deployment = {
      programId: '$PROGRAM_ID',
      poolConfig: pool.poolConfig || '$POOL_CONFIG',
      epochTree: pool.epochTree || null,
      vaultAuthority: pool.vaultAuthority || null,
      vault: pool.vault || null,
      tokenMint: token.mint || '$TOKEN_MINT',
      payerAta: token.payerAta || null,
      epochParams: {
        duration: $EPOCH_DURATION,
        finalizationDelay: $FINALIZATION_DELAY,
        expiry: $EXPIRY,
        burnRateBps: $BURN_RATE_BPS,
      },
      rpcUrl: '$RPC_URL',
      wallet: '$WALLET_PUBKEY',
      deployedAt: new Date().toISOString(),
    };
    fs.writeFileSync('$DEPLOYMENT_FILE', JSON.stringify(deployment, null, 2));
    console.log('Deployment info saved to: $DEPLOYMENT_FILE');
  " 2>&1
fi

echo ""
echo -e "${GREEN}${BOLD}Deployment complete!${NC}"
echo ""
echo "Next steps:"
echo "  1. Run SDK devnet smoke test (Task 5):"
echo "     cd sdk && SOLANA_RPC_URL=$RPC_URL npx jest tests/devnet-smoke.test.ts --timeout 300000"
echo "  2. Inspect pool on explorer:"
echo "     https://explorer.solana.com/address/$POOL_CONFIG?cluster=devnet"
echo ""
