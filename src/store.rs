use std::io;
use std::path::Path;

use crate::internal;
use crate::internal::INDEX_ENTRY_SIZE_IN_BYTES;

pub struct Store {
    buffer_pool: internal::BufferPool,
    header: internal::DbFileHeader,
}

impl Store {
    /// Creates a new store instance for the db found at `store_path`
    pub fn new(
        store_path: &str,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        pool_capacity: Option<usize>,
    ) -> io::Result<Self> {
        let db_folder = Path::new(store_path);
        internal::fs::initialize_file_db(db_folder);
        let db_file_path = db_folder.join("dump.scdb");
        let mut buffer_pool = internal::BufferPool::new(
            pool_capacity,
            &db_file_path,
            max_keys,
            redundant_blocks,
            None,
        )?;
        let header: internal::DbFileHeader =
            internal::DbFileHeader::from_file(&mut buffer_pool.file)?;
        let store = Self {
            buffer_pool,
            header,
        };

        Ok(store)
    }

    pub fn close(&mut self) {
        // Flush the memory mapped file to disk
        // todo!()
    }

    /// Sets the given key value in the store
    pub fn set(&mut self, k: &[u8], v: &[u8], ttl: Option<u64>) -> io::Result<()> {
        let expiry = match ttl {
            None => 0u64,
            Some(expiry) => expiry,
        };

        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);

        while index_block <= self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes = self
                .buffer_pool
                .read_at(index_offset, INDEX_ENTRY_SIZE_IN_BYTES as usize)?;
            let entry_offset = u64::from_be_bytes(internal::extract_array(&kv_offset_in_bytes)?);

            if entry_offset == 0 || self.buffer_pool.addr_belongs_to_key(entry_offset, k)? {
                let kv = internal::KeyValueEntry::new(k, v, expiry);
                let mut kv_bytes = kv.as_bytes();
                let prev_last_offset = self.buffer_pool.append(&mut kv_bytes)?;
                self.buffer_pool
                    .replace(index_offset, &prev_last_offset.to_be_bytes())?;
                self.header.last_offset = self.buffer_pool.file_size;
                return Ok(());
            }

            index_block += 1;
        }

        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("CollisionSaturatedError: no free slot for key: {:?}", k),
        ))
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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");

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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values);
        delete_keys(&mut store, &keys_to_delete);

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");

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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        store.clear();

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");

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
        let mut store = Store::new(STORE_PATH, None, None, None).expect("create store");

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
