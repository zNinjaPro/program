use anchor_lang::prelude::*;
use crate::state::{EpochTree, PoolTree, MERKLE_DEPTH, LEGACY_MERKLE_DEPTH};
use crate::errors::ShieldedPoolError;
use crate::field::reduce_bytes_to_field;
#[cfg_attr(not(target_os = "solana"), allow(unused_imports))]
use solana_poseidon::{hashv, Endianness, Parameters, PoseidonSyscallError};
use solana_program::{msg, program_error::ProgramError};

#[cfg(target_os = "solana")]
extern "C" {
    fn sol_poseidon(
        parameters: u64,
        endianness: u64,
        vals: *const u8,
        vals_len: u64,
        hash_result: *mut u8,
    ) -> u64;
}

pub fn hash_two(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    // Temporary diagnostic wrapper: avoid panics and log syscall errors.
    // TODO(prod): Change API to return Result, remove logging, and enforce strict input encoding at call sites.
    match poseidon_hash2(*left, *right) {
        Ok(h) => h,
        Err(e) => {
            msg!("hash_two failed: {:?}", e);
            // Return a sentinel zero; callers should treat this as failure.
            [0u8; 32]
        }
    }
}

/// Diagnostic wrapper around Poseidon 2-arity hash using DIRECT syscall.
/// Bypasses the hashv wrapper to get actual error codes from the syscall.
/// Logs detailed syscall errors and normalizes inputs to BN254 field.
///
/// Refactor guidance (production):
/// - Prefer returning `Result<[u8;32], MerkleError>` from public APIs.
/// - Remove logging here; rely on upstream error handling.
/// - Keep Endianness and parameter selections centralized.
fn poseidon_hash2(left: [u8; 32], right: [u8; 32]) -> Result<[u8; 32]> {
    let l = reduce_bytes_to_field(&left);
    let r = reduce_bytes_to_field(&right);
    
    // TEMPORARY DEBUG: Log first 4 bytes of each input to verify syscall args
    msg!("Poseidon inputs: left[0..4]={:?}, right[0..4]={:?}", &l[..4], &r[..4]);
    
    // Direct syscall invocation - bypassing hashv wrapper
    #[cfg(target_os = "solana")]
    {
        let mut hash_result = [0u8; 32];
        
        // Each input is represented as an (ptr, len) pair. Ensure 8-byte alignment.
        #[repr(C, align(8))]
        struct SliceDesc {
            ptr: u64,
            len: u64,
        }
        // Place descriptors on the stack to guarantee proper alignment.
        let descriptors: [SliceDesc; 2] = [
            SliceDesc { ptr: l.as_ptr() as u64, len: 32u64 },
            SliceDesc { ptr: r.as_ptr() as u64, len: 32u64 },
        ];
        
        let result = unsafe {
            sol_poseidon(
                0, // Parameters::Bn254X5 = 0
                0, // Endianness::BigEndian = 0
                descriptors.as_ptr() as *const u8,
                2, // 2 inputs
                hash_result.as_mut_ptr(),
            )
        };
        
        if result == 0 {
            msg!("Poseidon SUCCESS: hash[0..4]={:?}", &hash_result[..4]);
            Ok(hash_result)
        } else {
            let error = PoseidonSyscallError::from(result);
            msg!("Poseidon DIRECT syscall error code {}: {}", result, error);
            Err(ProgramError::InvalidArgument.into())
        }
    }
    
    // Fallback for non-Solana targets (tests)
    #[cfg(not(target_os = "solana"))]
    {
        let inputs: [&[u8]; 2] = [&l, &r];
        match hashv(Parameters::Bn254X5, Endianness::BigEndian, &inputs) {
            Ok(h) => {
                msg!("Poseidon success (non-Solana): hash[0..4]={:?}", &h.to_bytes()[..4]);
                Ok(h.to_bytes())
            },
            Err(e) => {
                msg!("Poseidon error (non-Solana): {}", e);
                Err(ProgramError::InvalidArgument.into())
            }
        }
    }
}

/// Zero hashes for Poseidon merkle tree (depth MERKLE_DEPTH = 12)
/// zero_hashes[0] = Poseidon(0)
/// zero_hashes[i] = Poseidon(zero_hashes[i-1], zero_hashes[i-1])
///
/// On-chain we keep precomputed constants (derived from Solana syscall); off-chain we
/// derive at runtime using the same hashing path to avoid drift in tests.
pub fn compute_zero_hashes(_depth: usize) -> [[u8; 32]; MERKLE_DEPTH] {
    #[cfg(target_os = "solana")]
    {
        // Precomputed zero hashes for depth 12 (first 12 entries)
        return [
            hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000000"),
            hex_literal::hex!("829a01fae4f8e22b1b4ca5ad5b54a5834ee098a77b735bd57431a7656d29a108"),
            hex_literal::hex!("50b4feaeb79752e57b182c6207a6984ebf5e6dc9d7e56c42889666509843b718"),
            hex_literal::hex!("f56fdd59a3fd78fbc066b31c20a0dc02d2fab63095664e87f2b2f0819e1cc22d"),
            hex_literal::hex!("6e58ea3b67b9d42ee340b22fcc79b87a8ce47a7a6d0404cb1d63fc16c0b95220"),
            hex_literal::hex!("2584ba0c4ab469e2d5d3c1e11b328a043f5cea0d1108539eec8c046b13bde31f"),
            hex_literal::hex!("c67b4a68ca203df0335e6fb6247a82963e5059ffa18e1af2cfb98581fea5aa00"),
            hex_literal::hex!("4dd60b46e179bc509022284c4ba37c9992b2e1b4f3261480dc18c2b346a9a01c"),
            hex_literal::hex!("4dc7695fdeb763e585c1fa1d235c42d196917acd8867cdcf20b5fca7594a3412"),
            hex_literal::hex!("363f05d4d2cca7b40d87546181acd14f1d21f9535c3d13c45dfbb32afaa3c516"),
            hex_literal::hex!("beab72b4311584a18d104dbf69ef69690840fd9fc40263b58122052478f08117"),
            hex_literal::hex!("e4f44df15cd40969d4f1bea1110ea66ba4e275ec3839ae243d72cd22f01f0d21"),
        ];
    }

    #[cfg(not(target_os = "solana"))]
    {
        let mut zeros = [[0u8; 32]; MERKLE_DEPTH];
        zeros[0] = [0u8; 32];
        for i in 1..MERKLE_DEPTH {
            zeros[i] = hash_two(&zeros[i - 1], &zeros[i - 1]);
        }
        return zeros;
    }
}

/// Legacy compute_zero_hashes for depth 32 (legacy PoolTree)
pub fn compute_zero_hashes_legacy(_depth: usize) -> [[u8; 32]; LEGACY_MERKLE_DEPTH] {
    #[cfg(target_os = "solana")]
    {
        // For legacy, we compute full 32 levels. On-chain we'd precompute all 32.
        // For simplicity, we'll compute them dynamically.
        let mut zeros = [[0u8; 32]; LEGACY_MERKLE_DEPTH];
        zeros[0] = [0u8; 32];
        for i in 1..LEGACY_MERKLE_DEPTH {
            zeros[i] = hash_two(&zeros[i - 1], &zeros[i - 1]);
        }
        return zeros;
    }

    #[cfg(not(target_os = "solana"))]
    {
        let mut zeros = [[0u8; 32]; LEGACY_MERKLE_DEPTH];
        zeros[0] = [0u8; 32];
        for i in 1..LEGACY_MERKLE_DEPTH {
            zeros[i] = hash_two(&zeros[i - 1], &zeros[i - 1]);
        }
        return zeros;
    }
}

// ============================================================================
// EpochTree Operations (New Epoch-Based System)
// ============================================================================

/// Insert a new leaf into an EpochTree
/// Returns the new root and the leaf index
pub fn insert_leaf_epoch(tree: &mut EpochTree, leaf: [u8; 32]) -> Result<(u64, [u8; 32])> {
    let depth = tree.depth as usize;
    let mut idx = tree.next_index;
    
    // Check if tree is full
    if idx >= (1u64 << depth) {
        return Err(ShieldedPoolError::TreeFull.into());
    }
    
    let leaf_index = idx;
    let mut cur_hash = leaf;
    let mut level = 0usize;
    
    // Climb the tree, updating frontier nodes
    while level < depth {
        if idx % 2 == 0 {
            // Left child: store in frontier
            tree.frontier[level] = cur_hash;
            // Continue climbing with zero hash as right sibling
            let parent = hash_two(&cur_hash, &tree.zero_hashes[level]);
            cur_hash = parent;
            idx >>= 1;
            level += 1;
        } else {
            // Right child: combine with left sibling from frontier
            let left = tree.frontier[level];
            let right = cur_hash;
            let parent = hash_two(&left, &right);
            cur_hash = parent;
            idx >>= 1;
            level += 1;
        }
    }
    
    // cur_hash is now the root
    let root = cur_hash;
    
    // Update root ring buffer - write at current head, then advance
    tree.roots[tree.roots_head as usize] = root;
    tree.roots_head = ((tree.roots_head as usize + 1) % tree.roots.len()) as u8;
    
    if (tree.roots_len as usize) < tree.roots.len() {
        tree.roots_len += 1;
    }
    
    // Increment next index
    tree.next_index += 1;
    
    Ok((leaf_index, root))
}

/// Check if a root exists in the EpochTree's root history
pub fn is_known_root_epoch(tree: &EpochTree, root: &[u8; 32]) -> bool {
    let len = tree.roots_len as usize;
    for i in 0..len {
        if &tree.roots[i] == root {
            return true;
        }
    }
    false
}

/// Compute the current root from the EpochTree's frontier
pub fn compute_root_epoch(tree: &EpochTree) -> [u8; 32] {
    let depth = tree.depth as usize;
    let mut idx = tree.next_index;
    
    if idx == 0 {
        // Empty tree - return zero hash at root level
        return tree.zero_hashes[depth - 1];
    }
    
    // Start from the last inserted leaf position
    idx -= 1;
    let mut cur_hash = tree.frontier[0];
    
    for level in 0..depth {
        if idx % 2 == 0 {
            cur_hash = hash_two(&cur_hash, &tree.zero_hashes[level]);
        } else {
            cur_hash = hash_two(&tree.frontier[level], &cur_hash);
        }
        idx >>= 1;
    }
    
    cur_hash
}

// ============================================================================
// Legacy PoolTree Operations (Preserved for compatibility)
// ============================================================================

/// Insert a new leaf into the frontier-only Merkle tree
/// Returns the new root and the leaf index
pub fn insert_leaf(tree: &mut PoolTree, leaf: [u8; 32]) -> Result<(u64, [u8; 32])> {
    let depth = tree.depth as usize;
    let mut idx = tree.next_index;
    
    // Check if tree is full
    if idx >= (1u64 << depth) {
        return Err(ShieldedPoolError::TreeFull.into());
    }
    
    let leaf_index = idx;
    let mut cur_hash = leaf;
    let mut level = 0usize;
    
    // Climb the tree, updating frontier nodes
    while level < depth {
        if idx % 2 == 0 {
            // Left child: store in frontier
            tree.frontier[level] = cur_hash;
            // Continue climbing with zero hash as right sibling
            let parent = hash_two(&cur_hash, &tree.zero_hashes[level]);
            cur_hash = parent;
            idx >>= 1;
            level += 1;
        } else {
            // Right child: combine with left sibling from frontier
            let left = tree.frontier[level];
            let right = cur_hash;
            let parent = hash_two(&left, &right);
            cur_hash = parent;
            idx >>= 1;
            level += 1;
        }
    }
    
    // cur_hash is now the root
    let root = cur_hash;
    
    // Update root ring buffer - write at current head, then advance
    tree.roots[tree.roots_head as usize] = root;
    tree.roots_head = ((tree.roots_head as usize + 1) % tree.roots.len()) as u8;
    
    if (tree.roots_len as usize) < tree.roots.len() {
        tree.roots_len += 1;
    }
    
    // Increment next index
    tree.next_index += 1;
    
    Ok((leaf_index, root))
}

/// Check if a root exists in the root history
pub fn is_known_root(tree: &PoolTree, root: &[u8; 32]) -> bool {
    let len = tree.roots_len as usize;
    for i in 0..len {
        if &tree.roots[i] == root {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_zero_hashes() {
        let zero_hashes = compute_zero_hashes(32);
        
        // First zero hash should be all zeros
        assert_eq!(zero_hashes[0], [0u8; 32]);
        
        // Each subsequent hash should be hash(prev, prev)
        for i in 1..32 {
            let expected = hash_two(&zero_hashes[i - 1], &zero_hashes[i - 1]);
            assert_eq!(zero_hashes[i], expected);
        }
    }
    
    #[test]
    fn test_insert_single_leaf() {
        let mut tree = PoolTree {
            depth: 4,
            _padding0: [0u8; 7],
            next_index: 0,
            frontier: [[0; 32]; LEGACY_MERKLE_DEPTH],
            roots: [[0; 32]; 64],
            roots_len: 0,
            roots_head: 0,
            zero_hashes: compute_zero_hashes_legacy(LEGACY_MERKLE_DEPTH),
            _padding_end: [0u8; 6],
        };
        
        let leaf = [1u8; 32];
        let result = insert_leaf(&mut tree, leaf);
        assert!(result.is_ok());
        
        let (leaf_index, root) = result.unwrap();
        assert_eq!(leaf_index, 0);
        assert_eq!(tree.next_index, 1);
        assert_eq!(tree.roots_len, 1);
        assert_eq!(tree.roots_head, 1);
        assert_eq!(tree.roots[0], root);
    }
    
    #[test]
    fn test_insert_multiple_leaves() {
        let mut tree = PoolTree {
            depth: 4,
            _padding0: [0u8; 7],
            next_index: 0,
            frontier: [[0; 32]; LEGACY_MERKLE_DEPTH],
            roots: [[0; 32]; 64],
            roots_len: 0,
            roots_head: 0,
            zero_hashes: compute_zero_hashes_legacy(LEGACY_MERKLE_DEPTH),
            _padding_end: [0u8; 6],
        };
        
        // Insert 5 leaves
        let mut roots = vec![];
        for i in 0..5 {
            let leaf = [i as u8; 32];
            let result = insert_leaf(&mut tree, leaf);
            assert!(result.is_ok());
            
            let (leaf_index, root) = result.unwrap();
            assert_eq!(leaf_index, i as u64);
            roots.push(root);
        }
        
        assert_eq!(tree.next_index, 5);
        assert_eq!(tree.roots_len, 5);
        
        // All roots should be in history
        for root in roots {
            assert!(is_known_root(&tree, &root));
        }
    }
    
    #[test]
    fn test_root_ring_buffer_overflow() {
        let mut tree = PoolTree {
            depth: 8, // 2^8 = 256 leaves capacity
            _padding0: [0u8; 7],
            next_index: 0,
            frontier: [[0; 32]; LEGACY_MERKLE_DEPTH],
            roots: [[0; 32]; 64],
            roots_len: 0,
            roots_head: 0,
            zero_hashes: compute_zero_hashes_legacy(LEGACY_MERKLE_DEPTH),
            _padding_end: [0u8; 6],
        };
        
        // Insert more than ROOT_HISTORY leaves
        for i in 0..70 {
            let leaf = [i as u8; 32];
            let _ = insert_leaf(&mut tree, leaf);
        }
        
        // roots_len should cap at 64
        assert_eq!(tree.roots_len, 64);
        
        // roots_head should wrap around (70 insertions means head moved 70 times)
        assert_eq!(tree.roots_head, (70 % 64) as u8);
    }
    
    #[test]
    fn test_tree_full() {
        let mut tree = PoolTree {
            depth: 2, // Only 4 leaves max
            _padding0: [0u8; 7],
            next_index: 0,
            frontier: [[0; 32]; LEGACY_MERKLE_DEPTH],
            roots: [[0; 32]; 64],
            roots_len: 0,
            roots_head: 0,
            zero_hashes: compute_zero_hashes_legacy(LEGACY_MERKLE_DEPTH),
            _padding_end: [0u8; 6],
        };
        
        // Fill the tree
        for i in 0..4 {
            let leaf = [i as u8; 32];
            assert!(insert_leaf(&mut tree, leaf).is_ok());
        }
        
        // Next insert should fail
        let leaf = [99u8; 32];
        let result = insert_leaf(&mut tree, leaf);
        assert!(result.is_err());
    }
}
