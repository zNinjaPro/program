#!/usr/bin/env node
/**
 * Create a test SPL token for local development and testing.
 * This script creates a simple SPL token with mint authority retained
 * for testing purposes.
 *
 * Usage: node program/scripts/create-test-token.js [INITIAL_SUPPLY]
 *
 * INITIAL_SUPPLY: Amount of tokens to mint (default: 1,000,000,000)
 *                 This is the base amount before decimals (9 decimals)
 */

const fs = require("fs");
const path = require("path");
const os = require("os");
const anchor = require("@coral-xyz/anchor");
const {
  createMint,
  mintTo,
  getOrCreateAssociatedTokenAddress,
  TOKEN_PROGRAM_ID,
} = require("@solana/spl-token");

async function main() {
  const [supplyArg] = process.argv.slice(2);
  const initialSupply = supplyArg ? BigInt(supplyArg) : BigInt(1_000_000_000);
  const decimals = 9;

  console.log("=".repeat(60));
  console.log("Creating Test Token for Shielded Pool");
  console.log("=".repeat(60));

  // Build provider from environment
  const provider = buildProvider();
  anchor.setProvider(provider);

  const connection = provider.connection;
  const payer = provider.wallet.payer;

  console.log(`\nNetwork: ${connection.rpcEndpoint}`);
  console.log(`Payer: ${payer.publicKey.toBase58()}`);
  console.log(`Initial Supply: ${initialSupply.toLocaleString()} tokens`);
  console.log(`Decimals: ${decimals}`);

  // Check payer balance
  const balance = await connection.getBalance(payer.publicKey);
  console.log(`Payer Balance: ${balance / 1e9} SOL`);

  if (balance < 0.1 * 1e9) {
    console.error(
      "\n⚠️  Low balance! Need at least 0.1 SOL for token creation.",
    );
    console.log("Run: solana airdrop 2 --url localhost");
    process.exit(1);
  }

  // Create the mint
  console.log("\n📦 Creating token mint...");
  const mint = await createMint(
    connection,
    payer,
    payer.publicKey, // Mint authority (retained for testing)
    null, // Freeze authority (none)
    decimals,
    undefined,
    undefined,
    TOKEN_PROGRAM_ID,
  );

  console.log(`✅ Token Mint: ${mint.toBase58()}`);

  // Get or create ATA for payer
  console.log("\n📦 Creating associated token account...");
  const payerAta = await getOrCreateAssociatedTokenAddress(
    connection,
    payer,
    mint,
    payer.publicKey,
  );

  console.log(`✅ Payer ATA: ${payerAta.toBase58()}`);

  // Mint initial supply
  const mintAmount = initialSupply * BigInt(10 ** decimals);
  console.log(`\n📦 Minting ${initialSupply.toLocaleString()} tokens...`);

  await mintTo(connection, payer, mint, payerAta, payer, mintAmount);

  console.log(`✅ Minted ${initialSupply.toLocaleString()} tokens to payer`);

  // Save mint address for other scripts
  const outputPath = path.resolve(__dirname, "../test-token-mint.json");
  const outputData = {
    mint: mint.toBase58(),
    decimals,
    initialSupply: initialSupply.toString(),
    payerAta: payerAta.toBase58(),
    createdAt: new Date().toISOString(),
    network: connection.rpcEndpoint,
  };

  fs.writeFileSync(outputPath, JSON.stringify(outputData, null, 2));
  console.log(`\n📁 Saved mint info to: ${outputPath}`);

  // Print summary
  console.log("\n" + "=".repeat(60));
  console.log("TEST TOKEN CREATED SUCCESSFULLY");
  console.log("=".repeat(60));
  console.log(`\nMint Address: ${mint.toBase58()}`);
  console.log(`Decimals: ${decimals}`);
  console.log(`Total Supply: ${initialSupply.toLocaleString()}`);
  console.log(`\nTo initialize pool with this token:`);
  console.log(`  node program/scripts/init_pool_v2.js ${mint.toBase58()}`);
  console.log("\n" + "=".repeat(60));

  return mint.toBase58();
}

function buildProvider() {
  // Try to load keypair from default Solana config
  const keypairPath =
    process.env.ANCHOR_WALLET ||
    path.join(os.homedir(), ".config", "solana", "id.json");

  if (!fs.existsSync(keypairPath)) {
    console.error(`Keypair not found at ${keypairPath}`);
    console.error("Set ANCHOR_WALLET env var or run: solana-keygen new");
    process.exit(1);
  }

  const keypairData = JSON.parse(fs.readFileSync(keypairPath, "utf8"));
  const keypair = anchor.web3.Keypair.fromSecretKey(
    Uint8Array.from(keypairData),
  );

  const rpcUrl = process.env.ANCHOR_PROVIDER_URL || "http://localhost:8899";
  const connection = new anchor.web3.Connection(rpcUrl, "confirmed");

  const wallet = new anchor.Wallet(keypair);
  return new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
}

main()
  .then((mint) => {
    process.exit(0);
  })
  .catch((err) => {
    console.error("\n❌ Error:", err.message);
    console.error(err);
    process.exit(1);
  });
