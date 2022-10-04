use crate::{dmap, fs};
use core::option::Option::{None, Some};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::io;
use twox_hash::XxHash64;

pub struct Store {
    db: HashMap<Vec<u8>, Vec<u8>, BuildHasherDefault<XxHash64>>,
    store_path: String,
}

impl Store {
    pub fn new(store_path: &str) -> Self {
        fs::initialize_file_db(store_path);
        let db = dmap::mmap::load_dmap_from_path(store_path);

        let mut store = Self {
            db,
            store_path: store_path.to_string(),
        };

        store
    }

    pub fn close(&mut self) {
        // Flush the memory mapped file to disk
        // todo!()
    }

    pub fn set(&mut self, k: &Vec<u8>, v: &Vec<u8>) -> io::Result<()> {
        self.db.insert(k.clone(), v.clone());
        Ok(())
    }

    pub fn get(&self, k: &Vec<u8>) -> io::Result<Option<Vec<u8>>> {
        let v = self.db.get(k).map(|v| v.clone());
        Ok(v)
    }

    pub fn delete(&mut self, k: &Vec<u8>) -> io::Result<()> {
        self.db.remove(k);
        Ok(())
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.db.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io;

    const STORE_PATH: &str = "db";

    #[test]
    #[serial]
    fn set_and_read_multiple_key_value_pairs() {
        let mut store = Store::new(STORE_PATH);
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
        let mut store = Store::new(STORE_PATH);
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
        let mut store = Store::new(STORE_PATH);
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
        let mut store = Store::new(STORE_PATH);
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH);

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
        let mut store = Store::new(STORE_PATH);
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values);
        delete_keys(&mut store, &keys_to_delete);

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH);

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
        let mut store = Store::new(STORE_PATH);
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values);
        store.clear();

        // Close the store

        store.close();

        // Open new store instance
        let mut store = Store::new(STORE_PATH);

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
        let mut store = Store::new(STORE_PATH);

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
    fn get_values_for_keys(
        store: &Store,
        keys: &Vec<Vec<u8>>,
    ) -> Vec<io::Result<Option<Vec<u8>>>> {
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
                .set(k, v)
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
