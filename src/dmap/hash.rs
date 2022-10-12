use twox_hash::xxh3::hash64;

/// Generates the hash of the key for the given block length
/// to get the position in the block that the given key corresponds to
///
/// # Example
///
/// ```
/// let length = 67788;
/// let key = "foo";
/// let value = 78;
///
/// let mut items: Vec<int64> = Vec::with_capacity(length);
/// items.resize(length, 0);
///
/// let hash = get_hash("foo", length);
/// items[hash] = value;
/// ```
///
pub(crate) fn get_hash(key: &str, block_length: u32) -> u32 {
    let hash = hash64(key.as_bytes());
    (hash % (block_length as u64)) as u32
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use crate::dmap::hash::get_hash;

    #[test]
    fn get_hash_generates_unique_hashes() {
        let block_size: u32 = 1289;
        let keys = vec!["fooo", "food", "bar", "Bargain", "Balance", "Z"];
        let mut hashed_map: HashMap<u32, String> = Default::default();
        for key in &keys {
            let hash = get_hash(key, block_size);
            assert!(hash <= block_size);
            hashed_map.insert(hash, key.to_string());
        }

        // if the hashes are truly unique,
        // the length of the map will be the same as that of the vector
        assert_eq!(hashed_map.len(), keys.len())
    }

    #[test]
    fn get_hash_always_generates_the_same_hash_for_same_key() {
        let block_size: u32 = 1289;
        let key = "fooo";
        let expected_hash = get_hash(key, block_size);
        for _ in 0..3 {
            assert_eq!(expected_hash, get_hash(key, block_size))
        }
    }
}
