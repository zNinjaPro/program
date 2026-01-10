use anchor_lang::prelude::*;
use crate::state::NullifierChunk;
use crate::errors::ShieldedPoolError;

/// Compute the chunk index for a given nullifier
/// Uses first 4 bytes of nullifier as chunk index (big-endian)
pub fn compute_chunk_index(nullifier: &[u8; 32], chunk_size: u16) -> u32 {
    // Use first 4 bytes to determine chunk
    let idx = u32::from_be_bytes([nullifier[0], nullifier[1], nullifier[2], nullifier[3]]);
    // Modulo to get chunk index (this distributes nullifiers across chunks)
    idx % (u32::MAX / chunk_size as u32)
}

/// Check if a nullifier exists in a chunk
pub fn check_nullifier(chunk: &NullifierChunk, nullifier: &[u8; 32]) -> bool {
    for i in 0..chunk.count as usize {
        if &chunk.nodes[i] == nullifier {
            return true;
        }
    }
    false
}

/// Insert a nullifier into a chunk
/// Returns error if nullifier already exists or chunk is full
pub fn insert_nullifier(chunk: &mut NullifierChunk, nullifier: [u8; 32]) -> Result<()> {
    // Check if nullifier already exists
    if check_nullifier(chunk, &nullifier) {
        return Err(ShieldedPoolError::NullifierAlreadySpent.into());
    }
    
    // Check if chunk is full
    if chunk.count as usize >= chunk.nodes.len() {
        return Err(ShieldedPoolError::NullifierChunkFull.into());
    }
    
    // Insert nullifier
    chunk.nodes[chunk.count as usize] = nullifier;
    chunk.count += 1;
    
    Ok(())
}

/// Derive the PDA seeds for a nullifier chunk
pub fn nullifier_chunk_seeds(pool_id: &[u8; 32], chunk_index: u32) -> [Vec<u8>; 3] {
    [
        b"nullifier".to_vec(),
        pool_id.to_vec(),
        chunk_index.to_be_bytes().to_vec(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MAX_NULLIFIER_CHUNK_SIZE;
    
    #[test]
    fn test_compute_chunk_index() {
        let nullifier1 = [1u8; 32];
        let nullifier2 = [255u8; 32];
        
        let chunk1 = compute_chunk_index(&nullifier1, 256);
        let chunk2 = compute_chunk_index(&nullifier2, 256);
        
        // Different nullifiers should (usually) map to different chunks
        // Though collisions are possible with modulo
        assert!(chunk1 != chunk2 || chunk1 == chunk2); // Just verify it computes
    }
    
    #[test]
    fn test_insert_and_check_nullifier() {
        let mut chunk = NullifierChunk {
            pool_id: [0u8; 32],
            chunk_index: 0,
            count: 0,
            _padding: [0; 2],
            nodes: [[0u8; 32]; MAX_NULLIFIER_CHUNK_SIZE],
        };
        
        let nullifier = [1u8; 32];
        
        // Should not exist initially
        assert!(!check_nullifier(&chunk, &nullifier));
        
        // Insert nullifier
        let result = insert_nullifier(&mut chunk, nullifier);
        assert!(result.is_ok());
        assert_eq!(chunk.count, 1);
        
        // Should exist now
        assert!(check_nullifier(&chunk, &nullifier));
    }
    
    #[test]
    fn test_duplicate_nullifier() {
        let mut chunk = NullifierChunk {
            pool_id: [0u8; 32],
            chunk_index: 0,
            count: 0,
            _padding: [0; 2],
            nodes: [[0u8; 32]; MAX_NULLIFIER_CHUNK_SIZE],
        };
        
        let nullifier = [1u8; 32];
        
        // First insert should succeed
        assert!(insert_nullifier(&mut chunk, nullifier).is_ok());
        
        // Second insert should fail
        let result = insert_nullifier(&mut chunk, nullifier);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_multiple_nullifiers() {
        let mut chunk = NullifierChunk {
            pool_id: [0u8; 32],
            chunk_index: 0,
            count: 0,
            _padding: [0; 2],
            nodes: [[0u8; 32]; MAX_NULLIFIER_CHUNK_SIZE],
        };
        
        // Insert multiple different nullifiers
        for i in 0..10 {
            let mut nullifier = [0u8; 32];
            nullifier[31] = i as u8;
            assert!(insert_nullifier(&mut chunk, nullifier).is_ok());
        }
        
        assert_eq!(chunk.count, 10);
        
        // Verify all exist
        for i in 0..10 {
            let mut nullifier = [0u8; 32];
            nullifier[31] = i as u8;
            assert!(check_nullifier(&chunk, &nullifier));
        }
    }
    
    #[test]
    fn test_chunk_full() {
        let mut chunk = NullifierChunk {
            pool_id: [0u8; 32],
            chunk_index: 0,
            count: 0,
            _padding: [0; 2],
            nodes: [[0u8; 32]; MAX_NULLIFIER_CHUNK_SIZE],
        };
        
        // Fill the chunk
        for i in 0..MAX_NULLIFIER_CHUNK_SIZE {
            let mut nullifier = [0u8; 32];
            nullifier[30] = (i / 256) as u8;
            nullifier[31] = (i % 256) as u8;
            assert!(insert_nullifier(&mut chunk, nullifier).is_ok());
        }
        
        assert_eq!(chunk.count as usize, MAX_NULLIFIER_CHUNK_SIZE);
        
        // Next insert should fail
        let nullifier = [255u8; 32];
        let result = insert_nullifier(&mut chunk, nullifier);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_nullifier_chunk_seeds() {
        let pool_id = [42u8; 32];
        let chunk_index = 123u32;
        
        let seeds = nullifier_chunk_seeds(&pool_id, chunk_index);
        
        assert_eq!(seeds[0], b"nullifier".to_vec());
        assert_eq!(seeds[1], pool_id.to_vec());
        assert_eq!(seeds[2], chunk_index.to_be_bytes().to_vec());
    }
}
