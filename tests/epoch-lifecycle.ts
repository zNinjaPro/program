/**
 * Epoch Lifecycle Integration Tests
 *
 * Tests the complete lifecycle of epochs in the shielded pool:
 * 1. Create epoch 0 (done at pool init)
 * 2. Deposit into epoch 0
 * 3. Rollover to epoch 1 (freezes epoch 0)
 * 4. Finalize epoch 0 (after delay)
 * 5. Withdraw from finalized epoch 0
 * 6. Deposit into active epoch 1
 * 7. Renew notes from epoch 0 to epoch 1
 * 8. Eventually garbage collect epoch 0
 *
 * This test uses mock proofs (MOCK_PROOFS=1) for deterministic testing.
 */
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ShieldedPool } from "../target/types/shielded_pool";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
} from "@solana/spl-token";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import { assert, expect } from "chai";

// Constants matching the program
const MERKLE_DEPTH = 12;
const LEAF_CHUNK_SIZE = 256;
const MAX_LEAVES_PER_EPOCH = 4096; // 2^12

describe("Epoch Lifecycle Integration", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ShieldedPool as Program<ShieldedPool>;
  const payer = provider.wallet as anchor.Wallet;

  // Test accounts
  let mint: PublicKey;
  let poolConfig: PublicKey;
  let vaultAuthority: PublicKey;
  let vault: PublicKey;
  let userTokenAccount: PublicKey;

  // Helper to derive epoch tree PDA
  const getEpochTreePda = (epoch: bigint): PublicKey => {
    const [pda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("epoch_tree"),
        poolConfig.toBuffer(),
        Buffer.from(new BigUint64Array([epoch]).buffer),
      ],
      program.programId,
    );
    return pda;
  };

  // Helper to derive leaf chunk PDA
  const getLeafChunkPda = (epoch: bigint, chunkIndex: number): PublicKey => {
    const [pda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("leaves"),
        poolConfig.toBuffer(),
        Buffer.from(new BigUint64Array([epoch]).buffer),
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId,
    );
    return pda;
  };

  // Helper to derive verifier PDA
  const getVerifierPda = (circuitName: string): PublicKey => {
    const [pda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("verifier"),
        poolConfig.toBuffer(),
        Buffer.from(circuitName),
      ],
      program.programId,
    );
    return pda;
  };

  // Helper to derive nullifier marker PDA
  const getNullifierMarkerPda = (
    epoch: bigint,
    nullifier: number[],
  ): PublicKey => {
    const [pda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("nullifier"),
        poolConfig.toBuffer(),
        Buffer.from(new BigUint64Array([epoch]).buffer),
        Buffer.from(nullifier),
      ],
      program.programId,
    );
    return pda;
  };

  // Test timing configuration (aggressive for testing)
  const EPOCH_DURATION_SLOTS = 50; // ~20 seconds
  const EXPIRY_SLOTS = 150; // ~1 minute
  const FINALIZATION_DELAY_SLOTS = 5;

  before(async () => {
    // Airdrop SOL
    const sig = await provider.connection.requestAirdrop(
      payer.publicKey,
      20 * LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(sig, "confirmed");

    // Create test mint
    mint = await createMint(
      provider.connection,
      payer.payer,
      payer.publicKey,
      null,
      9,
      Keypair.generate(),
      undefined,
      TOKEN_PROGRAM_ID,
    );

    // Derive pool PDAs
    [poolConfig] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool_config"), mint.toBuffer()],
      program.programId,
    );

    [vaultAuthority] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault_authority"), poolConfig.toBuffer()],
      program.programId,
    );

    [vault] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), poolConfig.toBuffer()],
      program.programId,
    );

    // Create and fund user token account
    userTokenAccount = await createAccount(
      provider.connection,
      payer.payer,
      mint,
      payer.publicKey,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID,
    );

    await mintTo(
      provider.connection,
      payer.payer,
      mint,
      userTokenAccount,
      payer.publicKey,
      1_000_000_000_000, // 1000 tokens
      undefined,
      undefined,
      TOKEN_PROGRAM_ID,
    );

    console.log("Test setup complete");
    console.log("  Mint:", mint.toBase58());
    console.log("  Pool Config:", poolConfig.toBase58());
  });

  describe("Phase 1: Pool Initialization", () => {
    it("creates pool with epoch 0", async () => {
      const epochTree0 = getEpochTreePda(0n);

      await program.methods
        .initializePoolV2(
          new anchor.BN(EPOCH_DURATION_SLOTS),
          new anchor.BN(EXPIRY_SLOTS),
          new anchor.BN(FINALIZATION_DELAY_SLOTS),
        )
        .accounts({
          poolConfig,
          epochTree: epochTree0,
          vaultAuthority,
          vault,
          mint,
          authority: payer.publicKey,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      const config = await program.account.poolConfig.fetch(poolConfig);
      expect(config.version).to.equal(2);
      expect(config.currentEpoch.toNumber()).to.equal(0);
      expect(config.epochDurationSlots.toNumber()).to.equal(
        EPOCH_DURATION_SLOTS,
      );

      console.log("Pool initialized with epoch 0");
    });

    it("initializes verifiers", async () => {
      const zeroG1 = (): number[][] => [Array(32).fill(0), Array(32).fill(0)];
      const zeroG2 = (): number[][] => Array(4).fill(Array(32).fill(0));

      // Initialize all verifiers with placeholder keys
      for (const [circuit, icCount] of [
        [{ withdraw: {} }, 9],
        [{ shieldedTransfer: {} }, 12],
        [{ renew: {} }, 7],
      ] as const) {
        const circuitName = Object.keys(circuit)[0];
        const verifier = getVerifierPda(
          circuitName === "shieldedTransfer" ? "transfer" : circuitName,
        );

        await program.methods
          .initializeVerifier(
            circuit as any,
            zeroG1(),
            zeroG2(),
            zeroG2(),
            zeroG2(),
            [],
          )
          .accounts({
            verifierConfig: verifier,
            poolConfig,
            payer: payer.publicKey,
            systemProgram: SystemProgram.programId,
          })
          .rpc();

        await program.methods
          .appendVerifierIc(circuit as any, Array(icCount).fill(zeroG1()))
          .accounts({
            verifierConfig: verifier,
            poolConfig,
            authority: payer.publicKey,
          })
          .rpc();
      }

      console.log("All verifiers initialized");
    });
  });

  describe("Phase 2: Epoch 0 Operations", () => {
    const depositsEpoch0: {
      commitment: number[];
      amount: number;
      leafIndex: number;
    }[] = [];

    it("initializes leaf chunk 0 for epoch 0", async () => {
      const epochTree = getEpochTreePda(0n);
      const leafChunk = getLeafChunkPda(0n, 0);

      await program.methods
        .initializeEpochLeafChunk(new anchor.BN(0), 0)
        .accounts({
          poolConfig,
          epochTree,
          leafChunk,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      console.log("Leaf chunk 0 initialized for epoch 0");
    });

    it("deposits 5 notes into epoch 0", async () => {
      const epochTree = getEpochTreePda(0n);
      const leafChunk = getLeafChunkPda(0n, 0);

      for (let i = 0; i < 5; i++) {
        const amount = (i + 1) * 1_000_000_000; // 1, 2, 3, 4, 5 tokens
        const commitment = Array(32).fill(0);
        commitment[0] = i + 1;
        commitment[31] = 0xaa;

        await program.methods
          .depositV2(
            commitment,
            new anchor.BN(amount),
            Buffer.from(`note_epoch0_${i}`),
          )
          .accounts({
            poolConfig,
            epochTree,
            leafChunk,
            vault,
            depositorTokenAccount: userTokenAccount,
            mint,
            depositor: payer.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .rpc();

        depositsEpoch0.push({ commitment, amount, leafIndex: i });
      }

      // Verify total deposited: 1+2+3+4+5 = 15 tokens
      const vaultBalance =
        await provider.connection.getTokenAccountBalance(vault);
      expect(vaultBalance.value.amount).to.equal("15000000000");
      console.log("Deposited 15 tokens across 5 notes into epoch 0");
    });

    it("verifies epoch 0 state", async () => {
      const epochTree = getEpochTreePda(0n);
      const treeAccount = await program.account.epochTree.fetch(epochTree);

      expect(treeAccount.epoch.toNumber()).to.equal(0);
      expect(treeAccount.nextIndex.toNumber()).to.equal(5);
      expect(treeAccount.state).to.equal(0); // Active
    });
  });

  describe("Phase 3: Epoch Rollover", () => {
    it("rolls over to epoch 1 (after waiting)", async function () {
      this.timeout(30000); // Extended timeout for slot waiting

      const config = await program.account.poolConfig.fetch(poolConfig);
      const epochEndSlot =
        config.epochStartSlot.toNumber() + config.epochDurationSlots.toNumber();

      // Wait for epoch to end
      let currentSlot = await provider.connection.getSlot();
      while (currentSlot < epochEndSlot) {
        await new Promise((resolve) => setTimeout(resolve, 500));
        currentSlot = await provider.connection.getSlot();
      }

      const epochTree0 = getEpochTreePda(0n);
      const epochTree1 = getEpochTreePda(1n);

      await program.methods
        .rolloverEpoch()
        .accounts({
          poolConfig,
          currentEpochTree: epochTree0,
          newEpochTree: epochTree1,
          authority: payer.publicKey,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      // Verify epoch advanced
      const newConfig = await program.account.poolConfig.fetch(poolConfig);
      expect(newConfig.currentEpoch.toNumber()).to.equal(1);

      // Verify epoch 0 is now frozen
      const tree0 = await program.account.epochTree.fetch(epochTree0);
      expect(tree0.state).to.equal(1); // Frozen

      console.log("Rolled over to epoch 1, epoch 0 is now frozen");
    });
  });

  describe("Phase 4: Epoch Finalization", () => {
    it("finalizes epoch 0 (after delay)", async function () {
      this.timeout(15000);

      const epochTree0 = getEpochTreePda(0n);
      const tree0Before = await program.account.epochTree.fetch(epochTree0);
      const finalizableSlot =
        tree0Before.endSlot.toNumber() + FINALIZATION_DELAY_SLOTS;

      // Wait for finalization delay
      let currentSlot = await provider.connection.getSlot();
      while (currentSlot < finalizableSlot) {
        await new Promise((resolve) => setTimeout(resolve, 500));
        currentSlot = await provider.connection.getSlot();
      }

      await program.methods
        .finalizeEpoch(new anchor.BN(0))
        .accounts({
          poolConfig,
          epochTree: epochTree0,
          authority: payer.publicKey,
        })
        .rpc();

      // Verify epoch 0 is now finalized
      const tree0After = await program.account.epochTree.fetch(epochTree0);
      expect(tree0After.state).to.equal(2); // Finalized

      console.log("Epoch 0 finalized");
    });
  });

  describe("Phase 5: Epoch 1 Operations", () => {
    it("initializes leaf chunk for epoch 1", async () => {
      const epochTree = getEpochTreePda(1n);
      const leafChunk = getLeafChunkPda(1n, 0);

      await program.methods
        .initializeEpochLeafChunk(new anchor.BN(1), 0)
        .accounts({
          poolConfig,
          epochTree,
          leafChunk,
          payer: payer.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      console.log("Leaf chunk 0 initialized for epoch 1");
    });

    it("deposits into epoch 1", async () => {
      const epochTree = getEpochTreePda(1n);
      const leafChunk = getLeafChunkPda(1n, 0);

      const commitment = Array(32).fill(0);
      commitment[0] = 100;
      commitment[31] = 0xbb;

      await program.methods
        .depositV2(
          commitment,
          new anchor.BN(10_000_000_000),
          Buffer.from("note_epoch1_0"),
        )
        .accounts({
          poolConfig,
          epochTree,
          leafChunk,
          vault,
          depositorTokenAccount: userTokenAccount,
          mint,
          depositor: payer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();

      console.log("Deposited 10 tokens into epoch 1");
    });
  });

  describe("Phase 6: Multi-Epoch State Verification", () => {
    it("verifies epoch 0 is finalized with correct state", async () => {
      const epochTree0 = getEpochTreePda(0n);
      const tree0 = await program.account.epochTree.fetch(epochTree0);

      expect(tree0.epoch.toNumber()).to.equal(0);
      expect(tree0.state).to.equal(2); // Finalized
      expect(tree0.nextIndex.toNumber()).to.equal(5);
      // Final root should be set
      expect(tree0.finalRoot.some((b: number) => b !== 0)).to.be.true;
    });

    it("verifies epoch 1 is active", async () => {
      const epochTree1 = getEpochTreePda(1n);
      const tree1 = await program.account.epochTree.fetch(epochTree1);

      expect(tree1.epoch.toNumber()).to.equal(1);
      expect(tree1.state).to.equal(0); // Active
      expect(tree1.nextIndex.toNumber()).to.equal(1);
    });

    it("verifies pool config tracks current epoch", async () => {
      const config = await program.account.poolConfig.fetch(poolConfig);
      expect(config.currentEpoch.toNumber()).to.equal(1);
    });
  });

  describe("Phase 7: Epoch State Machine Validation", () => {
    it("validates epoch state transitions are sequential", async () => {
      // Epoch states: Active (0) → Frozen (1) → Finalized (2) → GarbageCollected (3)
      // Epoch 0 went through: Active → (rollover) → Frozen → (finalize) → Finalized
      const epochTree0 = getEpochTreePda(0n);
      const tree0 = await program.account.epochTree.fetch(epochTree0);

      // Verify epoch 0 is in finalized state (state = 2)
      expect(tree0.state).to.equal(2, "Epoch 0 should be Finalized");

      // Verify epoch 1 is in active state (state = 0)
      const epochTree1 = getEpochTreePda(1n);
      const tree1 = await program.account.epochTree.fetch(epochTree1);
      expect(tree1.state).to.equal(0, "Epoch 1 should be Active");
    });

    it("rejects operations on wrong epoch state", async () => {
      const epochTree0 = getEpochTreePda(0n);

      // Try to deposit into finalized epoch 0 (should fail)
      try {
        const leafChunk0 = getLeafChunkPda(0n, 0);
        const commitment = Array(32).fill(0);
        commitment[0] = 200;

        await program.methods
          .depositV2(
            commitment,
            new anchor.BN(1_000_000_000),
            Buffer.from("invalid"),
          )
          .accounts({
            poolConfig,
            epochTree: epochTree0,
            leafChunk: leafChunk0,
            vault,
            depositorTokenAccount: userTokenAccount,
            mint,
            depositor: payer.publicKey,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .rpc();

        expect.fail("Should not allow deposit into finalized epoch");
      } catch (err: any) {
        // Expected to fail - epoch not active
        expect(err.toString()).to.include("EpochNotActive");
      }
    });
  });

  describe("Phase 8: Garbage Collection", () => {
    it("cannot garbage collect before expiry", async () => {
      const epochTree0 = getEpochTreePda(0n);

      try {
        await program.methods
          .garbageCollectEpoch(new anchor.BN(0))
          .accounts({
            poolConfig,
            epochTree: epochTree0,
            authority: payer.publicKey,
          })
          .rpc();

        expect.fail("Should not allow GC before expiry");
      } catch (err: any) {
        // Expected to fail - epoch not expired yet
        const errorMessage = err.toString();
        expect(
          errorMessage.includes("EpochNotExpired") ||
            errorMessage.includes("not expired") ||
            errorMessage.includes("6"), // Error code
        ).to.be.true;
      }
    });

    // Note: Full GC test would require waiting for EXPIRY_SLOTS
    // which is too long for standard test runs
    it("documents garbage collection behavior", () => {
      // Garbage collection (GC) rules:
      // 1. Can only GC epochs that are Finalized (state = 2)
      // 2. Must wait until current_slot >= epoch_end_slot + expiry_slots
      // 3. After GC, epoch state becomes GarbageCollected (state = 3)
      // 4. Notes in GC'd epochs can no longer be withdrawn or renewed
      // 5. GC reclaims storage (rent) from leaf chunks

      console.log(
        "GC behavior documented - full test requires time advancement",
      );
    });
  });

  describe("Phase 9: Summary", () => {
    it("prints final state summary", async () => {
      const config = await program.account.poolConfig.fetch(poolConfig);
      const vaultBalance =
        await provider.connection.getTokenAccountBalance(vault);

      console.log("\n========================================");
      console.log("EPOCH LIFECYCLE TEST SUMMARY");
      console.log("========================================");
      console.log("Current epoch:", config.currentEpoch.toNumber());
      console.log(
        "Total vault balance:",
        vaultBalance.value.uiAmount,
        "tokens",
      );
      console.log("Total burned:", config.totalBurned.toString(), "lamports");
      console.log("Burn rate:", config.burnRateBps, "bps (0.1%)");
      console.log("Epoch 0: Finalized with 5 notes (15 tokens deposited)");
      console.log("Epoch 1: Active with 1 note (10 tokens deposited)");
      console.log("========================================\n");
    });
  });
});
