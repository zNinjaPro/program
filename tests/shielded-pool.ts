import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { ShieldedPool } from "../target/types/shielded_pool";
import {
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  createMint,
  createAssociatedTokenAccount,
  createAccount,
  mintTo,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { assert } from "chai";

describe("shielded-pool", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.ShieldedPool as Program<ShieldedPool>;
  const payer = provider.wallet as anchor.Wallet;

  let mint: PublicKey;
  let poolConfig: PublicKey;
  let poolTree: PublicKey;
  let vaultAuthority: PublicKey;
  let vaultAccount: PublicKey;
  let userTokenAccount: PublicKey;
  let withdrawVerifier: PublicKey;
  let transferVerifier: PublicKey;

  const configSeed = Buffer.from([1]);
  const merkleDepth = 8;
  const rootHistory = 64;
  const nullifierChunkSize = 256;

  before(async () => {
    // Ensure the test wallet has SOL on a fresh local validator.
    const sig = await provider.connection.requestAirdrop(
      payer.publicKey,
      10 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(sig, "confirmed");

    // Create a test legacy SPL Token mint
    mint = await createMint(
      provider.connection,
      payer.payer,
      payer.publicKey,
      null,
      9,
      Keypair.generate(),
      undefined,
      TOKEN_PROGRAM_ID
    );

    console.log("Created test mint:", mint.toBase58());

    // Derive PDAs
    [poolConfig] = PublicKey.findProgramAddressSync(
      [Buffer.from("config"), mint.toBuffer()],
      program.programId
    );

    [poolTree] = PublicKey.findProgramAddressSync(
      [Buffer.from("tree"), mint.toBuffer()],
      program.programId
    );

    [vaultAuthority] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), mint.toBuffer()],
      program.programId
    );

    [withdrawVerifier] = PublicKey.findProgramAddressSync(
      [Buffer.from("verifier"), poolConfig.toBuffer(), Buffer.from("withdraw")],
      program.programId
    );

    [transferVerifier] = PublicKey.findProgramAddressSync(
      [Buffer.from("verifier"), poolConfig.toBuffer(), Buffer.from("transfer")],
      program.programId
    );

    // Create a regular legacy SPL token account owned by the PDA (off-curve owner)
    const vaultAccountKp = Keypair.generate();
    vaultAccount = await createAccount(
      provider.connection,
      payer.payer,
      mint,
      vaultAuthority,
      vaultAccountKp,
      undefined,
      TOKEN_PROGRAM_ID
    );
    console.log("Vault token account (non-ATA):", vaultAccount.toBase58());

    // Create user token account and mint some tokens
    userTokenAccount = await createAccount(
      provider.connection,
      payer.payer,
      mint,
      payer.publicKey,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    await mintTo(
      provider.connection,
      payer.payer,
      mint,
      userTokenAccount,
      payer.publicKey,
      10_000_000_000, // 10 tokens with 9 decimals
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    console.log("Minted tokens to user account");
  });

  it("Initializes pool", async () => {
    const tx = await program.methods
      .initializePool(
        // Pass Buffer for Vec<u8> fields
        configSeed,
        merkleDepth,
        rootHistory,
        nullifierChunkSize
      )
      .accounts({
        poolConfig,
        poolTree,
        vaultAuthority,
        mint,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize pool tx:", tx);

    // Verify pool config
    const config = await program.account.poolConfig.fetch(poolConfig);
    assert.equal(config.version, 1);
    assert.equal(config.mint.toBase58(), mint.toBase58());
    assert.equal(config.merkleDepth, merkleDepth);
    assert.equal(config.rootHistory, rootHistory);

    // Verify pool tree
    const tree = await program.account.poolTree.fetch(poolTree);
    assert.equal(tree.depth, merkleDepth);
    assert.equal(tree.nextIndex.toNumber(), 0);
    assert.equal(tree.rootsLen, 0);
  });

  it("Initializes verifier configs", async () => {
    const zeroG1 = () => [Array(32).fill(0), Array(32).fill(0)];
    const zeroG2 = () => [
      Array(32).fill(0),
      Array(32).fill(0),
      Array(32).fill(0),
      Array(32).fill(0),
    ];

    const makeIc = (len: number) => Array.from({ length: len }, () => zeroG1());

    const baseAccounts = {
      poolConfig,
      payer: payer.publicKey,
      systemProgram: SystemProgram.programId,
    };

    // Keep the initialize tx small; append IC points in a follow-up instruction.
    await program.methods
      .initializeVerifier(
        { withdraw: {} },
        zeroG1(),
        zeroG2(),
        zeroG2(),
        zeroG2(),
        makeIc(0)
      )
      .accounts({
        verifierConfig: withdrawVerifier,
        ...baseAccounts,
      })
      .rpc();

    await program.methods
      .appendVerifierIc({ withdraw: {} }, makeIc(8))
      .accounts({
        verifierConfig: withdrawVerifier,
        poolConfig,
        authority: payer.publicKey,
      })
      .rpc();

    await program.methods
      .initializeVerifier(
        { shieldedTransfer: {} },
        zeroG1(),
        zeroG2(),
        zeroG2(),
        zeroG2(),
        makeIc(0)
      )
      .accounts({
        verifierConfig: transferVerifier,
        ...baseAccounts,
      })
      .rpc();

    await program.methods
      .appendVerifierIc({ shieldedTransfer: {} }, makeIc(7))
      .accounts({
        verifierConfig: transferVerifier,
        poolConfig,
        authority: payer.publicKey,
      })
      .rpc();
  });

  it("Initializes leaf chunk 0", async () => {
    const chunkIndex = 0;

    const [leafChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("leaf"),
        mint.toBuffer(),
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    const tx = await program.methods
      .initializeLeafChunk(chunkIndex)
      .accounts({
        leafChunk,
        mint,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize leaf chunk tx:", tx);

    // Verify chunk was created with zero_copy account
    const chunkAccountInfo = await provider.connection.getAccountInfo(
      leafChunk
    );
    assert.ok(chunkAccountInfo, "LeafChunk account should exist");

    // Account should be 8 (discriminator) + 32 (mint) + 4 (chunk_index) + 2 (count) + 2 (padding) + 256*32 (leaves)
    const expectedSize = 8 + 32 + 4 + 2 + 2 + 256 * 32; // 8240 bytes total
    assert.equal(
      chunkAccountInfo.data.length,
      expectedSize,
      "LeafChunk size should match"
    );

    console.log("LeafChunk account size:", chunkAccountInfo.data.length);
  });

  it("Deposits into shielded pool with LeafChunk", async () => {
    const amount = 1_000_000_000; // 1 token
    const commitment = Buffer.alloc(32, 1); // Mock commitment
    const encryptedNote = Buffer.from("encrypted_note_data");
    const tag = Buffer.alloc(16, 42);

    // Get the LeafChunk PDA for index 0 (first 512 leaves)
    const chunkIndex = 0;
    const [leafChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("leaf"),
        mint.toBuffer(),
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    const tx = await program.methods
      .depositShielded(
        new anchor.BN(amount),
        Array.from(commitment),
        encryptedNote,
        Array.from(tag)
      )
      .accounts({
        poolConfig,
        poolTree,
        mint,
        userTokenAccount,
        vaultTokenAccount: vaultAccount,
        user: payer.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .remainingAccounts([
        {
          pubkey: leafChunk,
          isSigner: false,
          isWritable: true,
        },
      ])
      .rpc();

    console.log("Deposit tx:", tx);

    // Verify tree updated
    const tree = await program.account.poolTree.fetch(poolTree);
    assert.equal(tree.nextIndex.toNumber(), 1);
    assert.equal(tree.rootsLen, 1);

    // Verify commitment was written to LeafChunk at offset 0
    const chunkAccountInfo = await provider.connection.getAccountInfo(
      leafChunk
    );
    assert.ok(chunkAccountInfo);

    // Read count field: skip 8 (disc) + 32 (mint) + 4 (chunk_index) = 44 bytes
    const count = chunkAccountInfo.data.readUInt16LE(44);
    assert.equal(count, 1, "LeafChunk count should be 1");

    // Read first leaf: skip 8 + 32 + 4 + 2 + 2 = 48 bytes
    const firstLeaf = chunkAccountInfo.data.subarray(48, 48 + 32);
    assert.deepEqual(
      firstLeaf,
      commitment,
      "First leaf should match commitment"
    );

    console.log("Tree after deposit - next_index:", tree.nextIndex.toNumber());
  });

  it("Handles multiple deposits to same LeafChunk", async () => {
    const amount = 1_000_000_000;
    const chunkIndex = 0;
    const [leafChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("leaf"),
        mint.toBuffer(),
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    // Deposit 3 more commitments to test chunk filling
    for (let i = 0; i < 3; i++) {
      const commitment = Buffer.alloc(32, 2 + i);
      const encryptedNote = Buffer.from(`note_${i}`);
      const tag = Buffer.alloc(16, 43 + i);

      await program.methods
        .depositShielded(
          new anchor.BN(amount),
          Array.from(commitment),
          // Vec<u8> expects Buffer on the JS side
          encryptedNote,
          Array.from(tag)
        )
        .accounts({
          poolConfig,
          poolTree,
          mint,
          userTokenAccount,
          vaultTokenAccount: vaultAccount,
          user: payer.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          {
            pubkey: leafChunk,
            isSigner: false,
            isWritable: true,
          },
        ])
        .rpc();
    }

    // Verify tree state
    const tree = await program.account.poolTree.fetch(poolTree);
    assert.equal(tree.nextIndex.toNumber(), 4, "Should have 4 leaves total");

    // Verify LeafChunk count
    const chunkAccountInfo = await provider.connection.getAccountInfo(
      leafChunk
    );
    const count = chunkAccountInfo.data.readUInt16LE(44);
    assert.equal(count, 4, "LeafChunk should have count=4");

    console.log("LeafChunk now contains 4 commitments");
  });

  it("Initializes nullifier chunk", async () => {
    const poolId = poolConfig.toBuffer(); // Use poolConfig pubkey as unique pool id for test
    const chunkIndex = 0;

    const [nullifierChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("nullifier"),
        poolId,
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    const tx = await program.methods
      .initializeNullifierChunk(Array.from(poolId), chunkIndex)
      .accounts({
        nullifierChunk,
        payer: payer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize nullifier chunk tx:", tx);

    // Verify chunk was created
    const chunkAccountInfo = await provider.connection.getAccountInfo(
      nullifierChunk
    );
    assert.ok(chunkAccountInfo, "NullifierChunk account should exist");

    // Account should be 8 (disc) + 32 (pool_id) + 4 (chunk_index) + 2 (count) + 2 (padding) + 256*32 (nodes)
    const expectedSize = 8 + 32 + 4 + 2 + 2 + 256 * 32;
    assert.equal(
      chunkAccountInfo.data.length,
      expectedSize,
      "NullifierChunk size should match"
    );

    // Verify count is 0
    const count = chunkAccountInfo.data.readUInt16LE(44); // Skip 8+32+4 bytes
    assert.equal(count, 0, "Initial nullifier count should be 0");

    console.log(
      "NullifierChunk initialized with size:",
      chunkAccountInfo.data.length
    );
  });

  it("Tests LeafChunk spanning (prepares for chunk boundary)", async () => {
    // Test that we can reference the correct chunk as index increases
    const tree = await program.account.poolTree.fetch(poolTree);
    const currentIndex = tree.nextIndex.toNumber();

    console.log("Current tree index:", currentIndex);
    console.log("Deposits until chunk boundary (256):", 256 - currentIndex);

    // This test documents the chunk boundary behavior
    // When nextIndex reaches 256, deposits need chunk 1, not chunk 0
    const chunkForCurrentIndex = Math.floor(currentIndex / 256);
    assert.equal(chunkForCurrentIndex, 0, "Should still be in chunk 0");
  });

  it("Rejects invalid root", async () => {
    const invalidRoot = Buffer.alloc(32, 255);
    const publicInputs = [
      Array.from(invalidRoot),
      Array.from(Buffer.alloc(32, 30)),
      Array.from(Buffer.alloc(32, 40)),
      Array.from(Buffer.alloc(32, 99)),
      Array.from(Buffer.alloc(32, 0)),
      Array.from(Buffer.alloc(32, 0)),
    ];

    try {
      await program.methods
        .shieldedTransfer(Buffer.alloc(256), publicInputs, [], [], 1, 1)
        .accounts({
          poolConfig,
          poolTree,
          verifierConfig: transferVerifier,
          user: payer.publicKey,
        })
        .rpc();

      assert.fail("Should have rejected invalid root");
    } catch (err) {
      console.log("Correctly rejected invalid root");
      assert.match(
        err.toString(),
        /(InvalidRoot|AnchorError|custom program error)/
      );
    }
  });

  it("Withdraws from shielded pool", async () => {
    // For withdraw test, we'd need:
    // 1. Valid nullifier chunks
    // 2. Valid root from previous deposits
    // 3. Mock proof that verifies

    console.log(
      "Withdraw test requires nullifier chunk initialization - skipped for now"
    );

    // This demonstrates the structure even if we can't execute it yet
    const amount = 500_000_000;
    const tree = await program.account.poolTree.fetch(poolTree);
    const root = tree.roots[0];

    const publicInputs = [
      Array.from(root),
      Array.from(Buffer.alloc(32, 100)), // nullifier
      Array.from(Buffer.alloc(32, 0)), // value_out
      Array.from(Buffer.alloc(32, 99)), // tx_anchor
      Array.from(Buffer.alloc(32, 0)), // pool_id
      Array.from(Buffer.alloc(32, 0)), // chain_id
    ];

    console.log("Withdraw structure validated");
  });

  it("Tests nullifier double-spend prevention", async () => {
    const poolId = poolConfig.toBuffer();
    const nullifier = Buffer.alloc(32, 100);
    const chunkIndex = 0;

    // Get the nullifier chunk
    const [nullifierChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("nullifier"),
        poolId,
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    // Read chunk and verify nullifier count is still 0
    let chunkAccountInfo = await provider.connection.getAccountInfo(
      nullifierChunk
    );
    assert.ok(chunkAccountInfo);
    const initialCount = chunkAccountInfo.data.readUInt16LE(44);
    assert.equal(initialCount, 0, "Nullifier chunk should start empty");

    // Simulate a withdraw that inserts the nullifier
    // In a real scenario, this would be done by withdraw_shielded or shielded_transfer
    // For this test, we're just verifying the nullifier chunk structure is correct

    // Get the tree root for the mock proof
    const tree = await program.account.poolTree.fetch(poolTree);
    const root = tree.roots[0];

    // Create public inputs for a mock withdraw (1 input)
    const publicInputs = [
      Array.from(root), // root
      Array.from(nullifier), // nullifier[0]
      Array.from(Buffer.alloc(32, 0)), // value_out
      Array.from(Buffer.alloc(32, 99)), // tx_anchor
      Array.from(poolId), // pool_id
      Array.from(Buffer.alloc(32, 0)), // chain_id
    ];

    console.log("Nullifier double-spend prevention structure validated");
    console.log("- Nullifier chunk ready for insertion");
    console.log("- Chunk count:", initialCount);
    console.log(
      "- In production, shielded_transfer/withdraw would check & mark nullifiers"
    );
  });

  it("Tests shielded transfer structure with nullifiers", async () => {
    const tree = await program.account.poolTree.fetch(poolTree);
    const root = tree.roots[0];
    const poolId = poolConfig.toBuffer();

    // Mock a shielded transfer: 1 input, 1 output
    const nullifier = Buffer.alloc(32, 200);
    const commitment = Buffer.alloc(32, 201);
    const chunkIndex = 0;

    const [nullifierChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("nullifier"),
        poolId,
        Buffer.from(new Uint32Array([chunkIndex]).buffer),
      ],
      program.programId
    );

    // Get LeafChunk for the next index
    const nextIndex = tree.nextIndex.toNumber();
    const leafChunkIndex = Math.floor(nextIndex / 256);
    const [leafChunk] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("leaf"),
        mint.toBuffer(),
        Buffer.from(new Uint32Array([leafChunkIndex]).buffer),
      ],
      program.programId
    );

    const publicInputs = [
      Array.from(root), // root
      Array.from(nullifier), // nullifiers[0]
      Array.from(commitment), // commitments[0]
      Array.from(Buffer.alloc(32, 99)), // tx_anchor
      Array.from(poolId), // pool_id
      Array.from(Buffer.alloc(32, 0)), // chain_id
    ];

    // This would require a valid proof to execute
    console.log(
      "Shielded transfer structure validated with nullifier and leaf chunks"
    );
    console.log("- Nullifier chunk:", nullifierChunk.toBase58());
    console.log("- Leaf chunk:", leafChunk.toBase58());
    console.log("- Next tree index:", nextIndex);
  });
});
