#!/usr/bin/env node
/**
 * Initialize a v2 epoch-based shielded pool with token gating and burn.
 *
 * Usage: node program/scripts/init_pool_v2.js <MINT_PUBKEY> [BURN_RATE_BPS] [EPOCH_DURATION] [EXPIRY_SLOTS] [FINALIZATION_DELAY]
 *
 * BURN_RATE_BPS: Burn rate in basis points (default: 10 = 0.1%, max: 1000 = 10%)
 * EPOCH_DURATION: Epoch duration in slots (default: 0 = use program default ~2 weeks)
 * EXPIRY_SLOTS: Time before garbage collection allowed (default: 0 = use program default ~6 months)
 * FINALIZATION_DELAY: Delay before epoch can be finalized (default: 0 = use program default ~1 day)
 */

const fs = require("fs");
const path = require("path");
const os = require("os");
const anchor = require("@coral-xyz/anchor");
const { TOKEN_PROGRAM_ID } = require("@solana/spl-token");

function usage() {
  console.log(
    "Usage: node program/scripts/init_pool_v2.js <MINT_PUBKEY> [BURN_RATE_BPS=10] [EPOCH_DURATION=0] [EXPIRY_SLOTS=0] [FINALIZATION_DELAY=0]",
  );
  console.log(
    "\n  BURN_RATE_BPS: 10 = 0.1% burn on deposit/withdraw (max 1000 = 10%)",
  );
}

async function main() {
  const [mintArg, burnRateArg, epochDurationArg, expiryArg, finalizationArg] =
    process.argv.slice(2);

  if (!mintArg) {
    usage();
    process.exit(1);
  }

  const mint = new anchor.web3.PublicKey(mintArg);
  const burnRateBps = burnRateArg ? Number(burnRateArg) : 10; // Default 0.1%
  const epochDurationSlots = epochDurationArg
    ? BigInt(epochDurationArg)
    : BigInt(0);
  const expirySlots = expiryArg ? BigInt(expiryArg) : BigInt(0);
  const finalizationDelaySlots = finalizationArg
    ? BigInt(finalizationArg)
    : BigInt(0);

  // Validate burn rate
  if (!Number.isInteger(burnRateBps) || burnRateBps < 0 || burnRateBps > 1000) {
    throw new Error(
      `Invalid burn rate ${burnRateBps}. Must be 0-1000 basis points.`,
    );
  }

  console.log("=".repeat(60));
  console.log("Initializing Epoch-Based Shielded Pool V2");
  console.log("=".repeat(60));

  const provider = buildProvider();
  anchor.setProvider(provider);

  const idlPath = path.resolve(__dirname, "../target/idl/shielded_pool.json");
  if (!fs.existsSync(idlPath)) {
    throw new Error(`IDL not found at ${idlPath}. Run 'anchor build' first.`);
  }
  const idl = normalizeIdl(JSON.parse(fs.readFileSync(idlPath, "utf8")));
  const program = new anchor.Program(idl, provider);

  // Derive PDAs for v2 pool
  const [poolConfig] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("pool_config"), mint.toBuffer()],
    program.programId,
  );

  const [epochTree] = anchor.web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from("epoch_tree"),
      poolConfig.toBuffer(),
      Buffer.from(new BigUint64Array([BigInt(0)]).buffer),
    ],
    program.programId,
  );

  const [vaultAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("vault_authority"), poolConfig.toBuffer()],
    program.programId,
  );

  const [vault] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), poolConfig.toBuffer()],
    program.programId,
  );

  const payerPk = provider.wallet.publicKey;
  const lamports = await provider.connection.getBalance(payerPk);

  console.log(`\nNetwork: ${provider.connection.rpcEndpoint}`);
  console.log(`Payer: ${payerPk.toBase58()}`);
  console.log(`Balance: ${lamports / 1e9} SOL`);

  if (lamports === 0) {
    throw new Error(`Payer ${payerPk.toBase58()} has zero lamports`);
  }

  console.log("\n📋 Pool Configuration:");
  console.log(`  Mint: ${mint.toBase58()}`);
  console.log(`  Pool Config PDA: ${poolConfig.toBase58()}`);
  console.log(`  Epoch Tree PDA: ${epochTree.toBase58()}`);
  console.log(`  Vault Authority PDA: ${vaultAuthority.toBase58()}`);
  console.log(`  Vault PDA: ${vault.toBase58()}`);
  console.log(`  Burn Rate: ${burnRateBps} bps (${burnRateBps / 100}%)`);
  console.log(
    `  Epoch Duration: ${epochDurationSlots === BigInt(0) ? "default (~2 weeks)" : epochDurationSlots + " slots"}`,
  );
  console.log(
    `  Expiry Slots: ${expirySlots === BigInt(0) ? "default (~6 months)" : expirySlots + " slots"}`,
  );
  console.log(
    `  Finalization Delay: ${finalizationDelaySlots === BigInt(0) ? "default (~1 day)" : finalizationDelaySlots + " slots"}`,
  );

  console.log("\n📦 Submitting initialize_pool_v2 transaction...");

  const method = program.methods
    .initializePoolV2(
      new anchor.BN(epochDurationSlots.toString()),
      new anchor.BN(expirySlots.toString()),
      new anchor.BN(finalizationDelaySlots.toString()),
      burnRateBps,
    )
    .accounts({
      poolConfig,
      epochTree,
      vaultAuthority,
      vault,
      mint,
      authority: payerPk,
      payer: payerPk,
      systemProgram: anchor.web3.SystemProgram.programId,
      tokenProgram: TOKEN_PROGRAM_ID,
    });

  const payerSigner = provider.wallet.payer;
  if (payerSigner) {
    method.signers([payerSigner]);
  }

  const txSig = await method.rpc();

  console.log(`\n✅ Pool initialized successfully!`);
  console.log(`   Transaction: ${txSig}`);

  // Save pool info
  const outputPath = path.resolve(__dirname, "../pool-config.json");
  const outputData = {
    poolConfig: poolConfig.toBase58(),
    epochTree: epochTree.toBase58(),
    vaultAuthority: vaultAuthority.toBase58(),
    vault: vault.toBase58(),
    mint: mint.toBase58(),
    burnRateBps,
    createdAt: new Date().toISOString(),
    network: provider.connection.rpcEndpoint,
    txSignature: txSig,
  };

  fs.writeFileSync(outputPath, JSON.stringify(outputData, null, 2));
  console.log(`   Config saved to: ${outputPath}`);

  console.log("\n" + "=".repeat(60));
  console.log("POOL READY FOR DEPOSITS");
  console.log("=".repeat(60));
  console.log(`\nUsers must hold ${mint.toBase58()} to transact.`);
  console.log(`${burnRateBps / 100}% of each deposit/withdraw will be burned.`);
  console.log("\n" + "=".repeat(60));
}

function buildProvider() {
  const rpcUrl =
    process.env.ANCHOR_PROVIDER_URL ||
    process.env.SOLANA_RPC_URL ||
    "http://127.0.0.1:8899";

  const walletPath =
    process.env.ANCHOR_WALLET ||
    path.join(os.homedir(), ".config", "solana", "id.json");

  if (!fs.existsSync(walletPath)) {
    throw new Error(
      `Wallet file not found at ${walletPath}. Set ANCHOR_WALLET or create ~/.config/solana/id.json`,
    );
  }

  const secret = JSON.parse(fs.readFileSync(walletPath, "utf8"));
  const keypair = anchor.web3.Keypair.fromSecretKey(Uint8Array.from(secret));
  const wallet = new anchor.Wallet(keypair);
  const connection = new anchor.web3.Connection(rpcUrl, "confirmed");
  return new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
}

function normalizeIdl(idl) {
  const clone = { ...idl };
  if (Array.isArray(idl.accounts)) {
    clone.accounts = idl.accounts.map((account) => ({
      ...account,
      size: account.size ?? 0,
    }));
  }
  return clone;
}

main().catch(async (err) => {
  console.error("\n❌ Failed to initialize pool:", err.message);
  if (typeof err.getLogs === "function") {
    try {
      const logs = await err.getLogs();
      if (logs) {
        console.error("Transaction logs:\n", logs.join("\n"));
      }
    } catch (logErr) {
      // ignore
    }
  }
  process.exit(1);
});
