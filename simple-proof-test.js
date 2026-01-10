/**
 * Simple proof generation test (no Anchor framework)
 * Tests that circuit artifacts work and proofs can be generated
 */

const snarkjs = require("snarkjs");
const fs = require("fs");
const path = require("path");

async function test() {
  console.log("🔍 Testing proof generation...\n");

  const sdkCircuitsPath = path.join(__dirname, "../sdk/circuits");
  const wasmPath = path.join(sdkCircuitsPath, "withdraw.wasm");
  const zkeyPath = path.join(sdkCircuitsPath, "withdraw_final.zkey");

  // Check if files exist
  if (!fs.existsSync(wasmPath)) {
    console.log("❌ withdraw.wasm not found at:", wasmPath);
    console.log("   Run: cd ../circuits && ./copy-to-sdk.sh");
    process.exit(1);
  }

  if (!fs.existsSync(zkeyPath)) {
    console.log("❌ withdraw_final.zkey not found at:", zkeyPath);
    console.log("   Run: cd ../circuits && ./copy-to-sdk.sh");
    process.exit(1);
  }

  console.log("✅ Circuit artifacts found");
  console.log("   WASM:", wasmPath);
  console.log("   ZKEY:", zkeyPath);

  // Load test input
  const inputPath = path.join(
    __dirname,
    "../circuits/build/withdraw/input.json"
  );

  if (!fs.existsSync(inputPath)) {
    console.log("❌ Test input not found at:", inputPath);
    console.log("   Run: cd ../circuits && npm run test:withdraw");
    process.exit(1);
  }

  const input = JSON.parse(fs.readFileSync(inputPath, "utf8"));
  console.log("\n📝 Test inputs:");
  console.log("   merkleRoot:", input.merkleRoot.slice(0, 16) + "...");
  console.log("   nullifier:", input.nullifier.slice(0, 16) + "...");
  console.log("   recipient:", input.recipient.slice(0, 16) + "...");
  console.log("   amount:", input.amount);

  // Generate proof
  console.log("\n⚡ Generating proof...");
  const startTime = Date.now();

  const { proof, publicSignals } = await snarkjs.groth16.fullProve(
    input,
    wasmPath,
    zkeyPath
  );

  const duration = Date.now() - startTime;
  console.log(`✅ Proof generated in ${duration}ms\n`);

  // Format proof for Solana
  console.log("📦 Proof structure:");
  console.log("   pi_a[0]:", proof.pi_a[0].slice(0, 20) + "...");
  console.log("   pi_a[1]:", proof.pi_a[1].slice(0, 20) + "...");
  console.log("   pi_b[0][0]:", proof.pi_b[0][0].slice(0, 20) + "...");
  console.log("   pi_b[0][1]:", proof.pi_b[0][1].slice(0, 20) + "...");
  console.log("   pi_b[1][0]:", proof.pi_b[1][0].slice(0, 20) + "...");
  console.log("   pi_b[1][1]:", proof.pi_b[1][1].slice(0, 20) + "...");
  console.log("   pi_c[0]:", proof.pi_c[0].slice(0, 20) + "...");
  console.log("   pi_c[1]:", proof.pi_c[1].slice(0, 20) + "...");

  console.log("\n🔢 Public signals:");
  publicSignals.forEach((sig, i) => {
    const labels = ["merkleRoot", "nullifier", "recipient", "amount"];
    console.log(
      `   [${i}] ${labels[i] || "unknown"}:`,
      sig.slice(0, 20) + "..."
    );
  });

  // Convert to bytes for Solana
  const proofBytes = new Uint8Array(256);
  let offset = 0;

  // pi_a (64 bytes)
  for (const val of proof.pi_a.slice(0, 2)) {
    const hex = BigInt(val).toString(16).padStart(64, "0");
    const bytes = Buffer.from(hex, "hex");
    proofBytes.set(bytes, offset);
    offset += 32;
  }

  // pi_b (128 bytes) - note the order for BN254
  const pi_b_ordered = [
    proof.pi_b[0][1],
    proof.pi_b[0][0],
    proof.pi_b[1][1],
    proof.pi_b[1][0],
  ];
  for (const val of pi_b_ordered) {
    const hex = BigInt(val).toString(16).padStart(64, "0");
    const bytes = Buffer.from(hex, "hex");
    proofBytes.set(bytes, offset);
    offset += 32;
  }

  // pi_c (64 bytes)
  for (const val of proof.pi_c.slice(0, 2)) {
    const hex = BigInt(val).toString(16).padStart(64, "0");
    const bytes = Buffer.from(hex, "hex");
    proofBytes.set(bytes, offset);
    offset += 32;
  }

  console.log("\n✅ Proof formatted for Solana (256 bytes)");
  console.log(
    "   First 32 bytes:",
    Buffer.from(proofBytes.slice(0, 32)).toString("hex").slice(0, 32) + "..."
  );
  console.log(
    "   Last 32 bytes:",
    Buffer.from(proofBytes.slice(224, 256)).toString("hex").slice(0, 32) + "..."
  );

  console.log("\n✅ All tests passed!");
  console.log("   Circuit artifacts are valid");
  console.log("   Proof generation works");
  console.log("   Ready for on-chain integration");
}

test().catch((err) => {
  console.error("\n❌ Error:", err.message);
  process.exit(1);
});
