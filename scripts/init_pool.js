#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const os = require("os");
const anchor = require("@coral-xyz/anchor");

function usage() {
  console.log(
    "Usage: node program/scripts/init_pool.js <MINT_PUBKEY> [MERKLE_DEPTH=32] [ROOT_HISTORY=64] [NULLIFIER_CHUNK_SIZE=256]"
  );
}

async function main() {
  const [mintArg, depthArg, historyArg, chunkArg] = process.argv.slice(2);
  if (!mintArg) {
    usage();
    process.exit(1);
  }

  const mint = new anchor.web3.PublicKey(mintArg);
  const merkleDepth = depthArg ? Number(depthArg) : 32;
  const rootHistory = historyArg ? Number(historyArg) : 64;
  const nullifierChunkSize = chunkArg ? Number(chunkArg) : 256;

  if (!Number.isInteger(merkleDepth) || merkleDepth <= 0 || merkleDepth > 32) {
    throw new Error(`Invalid merkle depth ${merkleDepth}`);
  }
  if (
    !Number.isInteger(rootHistory) ||
    rootHistory <= 0 ||
    rootHistory > 1024
  ) {
    throw new Error(`Invalid root history ${rootHistory}`);
  }
  if (!Number.isInteger(nullifierChunkSize) || nullifierChunkSize <= 0) {
    throw new Error(`Invalid nullifier chunk size ${nullifierChunkSize}`);
  }

  const provider = buildProvider();
  anchor.setProvider(provider);

  const idlPath = path.resolve(__dirname, "../target/idl/shielded_pool.json");
  const idl = normalizeIdl(JSON.parse(fs.readFileSync(idlPath, "utf8")));
  const program = new anchor.Program(idl, provider);

  const [poolConfig] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config"), mint.toBuffer()],
    program.programId
  );
  const [poolTree] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("tree"), mint.toBuffer()],
    program.programId
  );
  const [vaultAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), mint.toBuffer()],
    program.programId
  );

  const payerPk = provider.wallet.publicKey;
  const lamports = await provider.connection.getBalance(payerPk);
  if (lamports === 0) {
    throw new Error(
      `Payer ${payerPk.toBase58()} has zero lamports on ${
        provider.connection.rpcEndpoint
      }`
    );
  }

  console.log("Submitting initialize_pool with args:");
  console.log("  mint:", mint.toBase58());
  console.log("  pool_config:", poolConfig.toBase58());
  console.log("  pool_tree:", poolTree.toBase58());
  console.log("  vault_authority:", vaultAuthority.toBase58());
  console.log("  merkle_depth:", merkleDepth);
  console.log("  root_history:", rootHistory);
  console.log("  nullifier_chunk_size:", nullifierChunkSize);

  const method = program.methods
    .initializePool(
      Buffer.alloc(0),
      merkleDepth,
      rootHistory,
      nullifierChunkSize
    )
    .accounts({
      poolConfig,
      poolTree,
      vaultAuthority,
      mint,
      payer: payerPk,
      systemProgram: anchor.web3.SystemProgram.programId,
    });

  const payerSigner = provider.wallet.payer;
  if (payerSigner) {
    method.signers([payerSigner]);
  }

  await method.rpc();
  console.log("✅ Pool initialized");
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
      `Wallet file not found at ${walletPath}. Set ANCHOR_WALLET or create ~/.config/solana/id.json`
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
  console.error("Failed to initialize pool:", err);
  if (typeof err.getLogs === "function") {
    try {
      const logs = await err.getLogs();
      if (logs) {
        console.error("Transaction logs:\n", logs.join("\n"));
      }
    } catch (logErr) {
      console.error("Failed to fetch logs:", logErr);
    }
  }
  process.exit(1);
});
