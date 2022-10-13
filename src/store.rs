use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::io;
use std::path::Path;

use memmap2::MmapMut;

use core::option::Option::{None, Some};

use crate::core::KeyValueEntry;
use crate::{core, fs};

pub struct Store {
    data_array: MmapMut,
    header: core::DbFileHeader,
    store_path: String,
}

impl Store {
    /// Creates a new store instance for the db found at `store_path`
    ///
    /// # Errors
    /// Returns errors if it fails to generate the mapping
    /// See [mmap::generate_mapping]
    ///
    /// [mmap::generate_mapping]: crate::core::mmap::generate_mapping
    pub fn new(
        store_path: &str,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
    ) -> io::Result<Self> {
        let db_folder = Path::new(store_path);
        fs::initialize_file_db(db_folder);
        let db_file_path = db_folder.join("dump.scdb");
        let data_array = core::generate_mapping(&db_file_path, max_keys, redundant_blocks)?;
        let header = core::DbFileHeader::from_data_array(&data_array)?;

        let store = Self {
            data_array,
            store_path: store_path.to_string(),
            header,
        };

        Ok(store)
    }

    pub fn close(&mut self) {
        // Flush the memory mapped file to disk
        // todo!()
    }

    pub fn set(&mut self, k: &Vec<u8>, v: &Vec<u8>, ttl: Option<u64>) -> io::Result<()> {
        let expiry = match ttl {
            None => 0u64,
            Some(expiry) => expiry,
        };

        let hash = core::get_hash(k, self.header.items_per_index_block) * 4;
        let header_offset = 100; // 100 bytes header
        let mut index_block_offset = 0u64;
        let mut index_address = (index_block_offset + header_offset + hash) as usize;
        let entry_offset = u32::from_be_bytes(core::extract_array::<4>(
            &self.data_array[index_address..(index_address + 4)],
        )?) as usize;

        if entry_offset == 0 {
            let kv = KeyValueEntry::new(k, v, expiry);
            let last_offset = self.header.last_offset;
            // FIXME: Consider using buffer pool management.
            // https://www1.columbia.edu/sec/acis/db2/db2d0/db2d0122.htm
            // https://www.ibm.com/docs/en/db2-for-zos/11?topic=storage-calculating-buffer-pool-size
            // self.data_array.(kv.get_byte_array());
            // self.data_array.inser;
        }

        // 2. Set `index_block_offset` to zero to start from the first block.
        //     3. The `index_address` is set to `index_block_offset + 801 + hash`.
        // 4. The 4-byte offset at the `index_address` offset is read. This is the first possible pointer to the key-value entry.
        //     Let's call it `key_value_offset`.
        // 5. If this `key_value_offset` is zero, this means that no value has been set for that key yet.
        //     - So the key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
        // , `value`, `deleted`) is appended to the end of the file at offset `last_offset`
        // - the `last_offset` is then inserted at `index_address` in place of the zero
        //     - the `last_offset` header is then updated
        // to `last_offset + get_size_of_kv(kv)` [get_size_of_kv gets the total size of the entry in bits]
        // 6. If this `key_value_offset` is non-zero, it is possible that the value for that key has already been set.
        //     - retrieve the key at the given `key_value_offset`. (Do note that there is a 4-byte number `key_size` before the
        // key. That number gives the size of the key).
        // - if this key is the same as the key passed, we have to update it by deleting then inserting it again:
        // - update the `deleted` of the key-value entry to 1
        //     - The key-value entry (with all its data including `key_size`, `expiry` (got from ttl from user), `value_size`
        // , `value`, `deleted`) is appended to the end of the file at offset `last_offset + 1`
        // - the `last_offset + 1` is then inserted at `index_address` in place of the former offset
        //     - the `last_offset` header is then updated to `last_offset + get_size_of_kv(kv)`
        // - else increment the `index_block_offset` by `net_block_size_in_bits`
        // - if the new `index_block_offset` is equal to or greater than the `key_values_start_point`, raise
        // the `CollisionSaturatedError` error. We have run out of blocks without getting a free slot to add the
        // key-value entry.
        //     - else go back to step 3.
        todo!()
    }

    pub fn get(&self, k: &Vec<u8>) -> io::Result<Option<Vec<u8>>> {
        todo!()
    }

    pub fn delete(&mut self, k: &Vec<u8>) -> io::Result<()> {
        todo!()
    }

    pub fn clear(&mut self) -> io::Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use serial_test::serial;

    use super::*;

    const STORE_PATH: &str = "db";

    #[test]
    #[serial]
    fn set_and_read_multiple_key_value_pairs() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        let received_values = get_values_for_keys(&store, &keys);

        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            values.iter().map(|v| Ok(Some(v.clone()))).collect();

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn set_and_delete_multiple_key_value_pairs() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values);
        delete_keys(&mut store, &keys_to_delete);

        let received_values = get_values_for_keys(&store, &keys);
        let mut expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            values[..2].iter().map(|v| Ok(Some(v.clone()))).collect();
        for _ in 0..keys_to_delete.len() {
            expected_values.push(Ok(None));
        }

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn set_and_clear() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        store.clear().expect("store cleared");

        let received_values = get_values_for_keys(&store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys.iter().map(|_| Ok(None)).collect();

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn persist_to_file() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");

        let received_values = get_values_for_keys(&store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            values.iter().map(|v| Ok(Some(v.clone()))).collect();

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn persist_to_file_after_delete() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values);
        delete_keys(&mut store, &keys_to_delete);

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");

        let received_values = get_values_for_keys(&store, &keys);
        let mut expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            values[..2].iter().map(|v| Ok(Some(v.clone()))).collect();
        for _ in 0..keys_to_delete.len() {
            expected_values.push(Ok(None));
        }

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn persist_to_file_after_clear() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        store.clear();

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");

        let received_values = get_values_for_keys(&store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys.iter().map(|_| Ok(None)).collect();

        for (got, expected) in received_values.into_iter().zip(expected_values) {
            assert_eq!(got.unwrap(), expected.unwrap());
        }

        store.close();
    }

    #[test]
    #[serial]
    fn close_flushes_the_memmapped_filed() {
        let mut store = Store::new(STORE_PATH, None, None).expect("create store");

        // Close the store
        store.close();

        todo!()
    }

    /// Deletes the given keys in the store
    fn delete_keys(store: &mut Store, keys_to_delete: &Vec<Vec<u8>>) {
        for k in keys_to_delete {
            store.delete(k).expect(&format!("delete key {:?}", k));
        }
    }

    /// Gets a vector of responses from the store when store.get is called
    /// for each key passed in keys
    fn get_values_for_keys(store: &Store, keys: &Vec<Vec<u8>>) -> Vec<io::Result<Option<Vec<u8>>>> {
        let mut received_values = Vec::with_capacity(keys.len());

        for k in keys {
            let _ = &received_values.push(store.get(k));
        }

        received_values
    }

    /// Inserts test data into the store
    fn insert_test_data(store: &mut Store, keys: &Vec<Vec<u8>>, values: &Vec<Vec<u8>>) {
        for (k, v) in keys.iter().zip(values) {
            store
                .set(k, v, None)
                .expect(&format!("set key {:?}, value {:?}", k, v));
        }
    }

    /// Gets keys for testing
    fn get_keys() -> Vec<Vec<u8>> {
        ["hey", "hi", "yoo-hoo", "bonjour"]
            .into_iter()
            .map(|v| v.to_string().into_bytes())
            .collect()
    }

    /// Gets values for testing
    fn get_values() -> Vec<Vec<u8>> {
        ["English", "English", "Slang", "French"]
            .into_iter()
            .map(|v| v.to_string().into_bytes())
            .collect()
    }
}
