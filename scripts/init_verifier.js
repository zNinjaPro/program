#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const os = require("os");
const anchor = require("@coral-xyz/anchor");

// Increase instruction encode buffer to handle larger IC vectors
const {
  BorshInstructionCoder,
} = require("@coral-xyz/anchor/dist/cjs/coder/borsh/instruction");
const BIG_INSTR_BUF = 4096;
const originalEncode = BorshInstructionCoder.prototype.encode;
BorshInstructionCoder.prototype.encode = function patchedEncode(ixName, ix) {
  const encoder = this.ixLayouts.get(ixName);
  if (!encoder) {
    throw new Error(`Unknown method: ${ixName}`);
  }
  const buffer = Buffer.alloc(BIG_INSTR_BUF);
  const len = encoder.layout.encode(ix, buffer);
  const data = buffer.slice(0, len);
  return Buffer.concat([Buffer.from(encoder.discriminator), data]);
};

const CIRCUIT_MAP = {
  withdraw: {
    variant: { withdraw: {} },
    seed: Buffer.from("withdraw"),
  },
  transfer: {
    variant: { transfer: {} },
    seed: Buffer.from("transfer"),
  },
  renew: {
    variant: { renew: {} },
    seed: Buffer.from("renew"),
  },
};

function usage() {
  console.log(
    "Usage: node program/scripts/init_verifier.js <withdraw|transfer|renew> <POOL_CONFIG_PUBKEY> [PATH_TO_VK_JSON]",
  );
}

function ensureArrayLength(arr, expected, label) {
  if (!Array.isArray(arr) || arr.length !== expected) {
    throw new Error(
      `${label} must have length ${expected}, got ${arr?.length}`,
    );
  }
}

function ensureBytes32(arr, label) {
  ensureArrayLength(arr, 32, label);
  arr.forEach((value, idx) => {
    if (typeof value !== "number" || value < 0 || value > 255) {
      throw new Error(`${label}[${idx}] must be a byte (0-255)`);
    }
  });
  return arr;
}

function normalizeG1(point, label) {
  ensureArrayLength(point, 2, label);
  return [
    ensureBytes32(point[0], `${label}.x`),
    ensureBytes32(point[1], `${label}.y`),
  ];
}

function normalizeG2(point, label) {
  ensureArrayLength(point, 4, label);
  return point.map((limb, idx) => ensureBytes32(limb, `${label}[${idx}]`));
}

async function main() {
  const [circuitArg, poolConfigArg, vkPathArg] = process.argv.slice(2);

  if (!circuitArg || !poolConfigArg) {
    usage();
    process.exit(1);
  }

  const circuit = CIRCUIT_MAP[circuitArg];
  if (!circuit) {
    console.error(
      `Invalid circuit "${circuitArg}". Expected withdraw, transfer, or renew.`,
    );
    usage();
    process.exit(1);
  }

  const poolConfigPk = new anchor.web3.PublicKey(poolConfigArg);
  const defaultVkPath = path.resolve(
    __dirname,
    "../verifier",
    `${circuitArg}_vk.json`,
  );
  const vkPath = vkPathArg
    ? path.resolve(process.cwd(), vkPathArg)
    : defaultVkPath;

  const payloadRaw = fs.readFileSync(vkPath, "utf8");
  const payload = JSON.parse(payloadRaw);

  const vkAlpha = normalizeG1(payload.vkAlpha, "vkAlpha");
  const vkBeta = normalizeG2(payload.vkBeta, "vkBeta");
  const vkGamma = normalizeG2(payload.vkGamma, "vkGamma");
  const vkDelta = normalizeG2(payload.vkDelta, "vkDelta");
  const icPoints = payload.icPoints.map((point, idx) =>
    normalizeG1(point, `icPoints[${idx}]`),
  );

  // Choose a small init chunk (or zero) to keep the first transaction tiny; append the rest later.
  const INIT_CHUNK = Number(process.env.INIT_IC_POINTS || 0);
  const APPEND_CHUNK = Number(process.env.APPEND_IC_CHUNK || 2);
  if (Number.isNaN(INIT_CHUNK) || INIT_CHUNK < 0) {
    throw new Error(
      `INIT_IC_POINTS must be a non-negative number, got ${process.env.INIT_IC_POINTS}`,
    );
  }
  if (Number.isNaN(APPEND_CHUNK) || APPEND_CHUNK <= 0) {
    throw new Error(
      `APPEND_IC_CHUNK must be a positive number, got ${process.env.APPEND_IC_CHUNK}`,
    );
  }
  const initIc = icPoints.slice(0, INIT_CHUNK);
  const remainingIc = icPoints.slice(INIT_CHUNK);

  const idlPath = path.resolve(__dirname, "../target/idl/shielded_pool.json");
  const rawIdl = JSON.parse(fs.readFileSync(idlPath, "utf8"));
  const idl = normalizeIdl(rawIdl);

  const provider = buildProvider();
  anchor.setProvider(provider);
  const program = new anchor.Program(idl, provider);

  const [verifierConfig] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("verifier"), poolConfigPk.toBuffer(), circuit.seed],
    program.programId,
  );

  const existing = await provider.connection.getAccountInfo(verifierConfig);
  if (existing) {
    console.log(
      `Verifier PDA ${verifierConfig.toBase58()} already exists. Delete it first if you need to reinitialize.`,
    );
    process.exit(0);
  }

  const payerPk = provider.wallet.publicKey;
  const lamports = await provider.connection.getBalance(payerPk);
  if (lamports === 0) {
    throw new Error(
      `Payer ${payerPk.toBase58()} has zero lamports on ${
        provider.connection.rpcEndpoint
      }`,
    );
  }

  console.log("Submitting initialize_verifier with args:");
  console.log("  circuit:", circuitArg);
  console.log("  pool_config:", poolConfigPk.toBase58());
  console.log("  init ic_points:", initIc.length, "/ total", icPoints.length);
  console.log(
    "  payer:",
    payerPk.toBase58(),
    "(balance",
    lamports,
    "lamports)",
  );

  const method = program.methods
    .initializeVerifier(
      circuit.variant,
      vkAlpha,
      vkBeta,
      vkGamma,
      vkDelta,
      initIc,
    )
    .accounts({
      verifierConfig,
      poolConfig: poolConfigPk,
      payer: provider.wallet.publicKey,
      systemProgram: anchor.web3.SystemProgram.programId,
    });

  const payerSigner = provider.wallet.payer;
  if (payerSigner) {
    method.signers([payerSigner]);
  }

  await method.rpc();
  console.log("✅ Verifier initialized at", verifierConfig.toBase58());

  // Stream the remaining IC points via append_verifier_ic to avoid TX size limits.
  let offset = INIT_CHUNK;
  while (remainingIc.length > 0) {
    const chunk = remainingIc.splice(0, APPEND_CHUNK);
    console.log(`Appending IC points ${offset}..${offset + chunk.length - 1}`);

    const appendIx = program.methods
      .appendVerifierIc(circuit.variant, chunk)
      .accounts({
        verifierConfig,
        poolConfig: poolConfigPk,
        authority: provider.wallet.publicKey,
      });

    if (payerSigner) {
      appendIx.signers([payerSigner]);
    }

    await appendIx.rpc();
    offset += chunk.length;
  }

  console.log("✅ All IC points appended; final length", icPoints.length);
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
  console.error("Failed to initialize verifier:", err);
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
