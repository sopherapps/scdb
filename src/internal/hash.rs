use twox_hash::xxh3::hash64;

/// Generates the hash of the key for the given block length
/// to get the position in the block that the given key corresponds to
pub(crate) fn get_hash(key: &[u8], block_length: u64) -> u64 {
    let hash = hash64(key);
    hash % block_length
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::internal::hash::get_hash;

    #[test]
    fn get_hash_generates_unique_hashes() {
        let block_size: u64 = 1289;
        let keys = vec!["fooo", "food", "bar", "Bargain", "Balance", "Z"];
        let mut hashed_map: HashMap<u64, String> = Default::default();
        for key in &keys {
            let hash = get_hash(key.as_bytes(), block_size);
            assert!(hash <= block_size);
            hashed_map.insert(hash, key.to_string());
        }

        // if the hashes are truly unique,
        // the length of the map will be the same as that of the vector
        assert_eq!(hashed_map.len(), keys.len())
    }

    #[test]
    fn get_hash_always_generates_the_same_hash_for_same_key() {
        let block_size: u64 = 1289;
        let key = "fooo";
        let expected_hash = get_hash(key.as_bytes(), block_size);
        for _ in 0..3 {
            assert_eq!(expected_hash, get_hash(key.as_bytes(), block_size))
        }
    }
}
