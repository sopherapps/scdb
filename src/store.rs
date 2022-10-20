use std::io;
use std::ops::DerefMut;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clokwerk::{ScheduleHandle, Scheduler, TimeUnits};

use crate::internal::{
    acquire_lock, get_current_timestamp, initialize_db_folder, slice_to_array, BufferPool,
    DbFileHeader, KeyValueEntry, INDEX_ENTRY_SIZE_IN_BYTES,
};

const DEFAULT_DB_FILE: &str = "dump.scdb";

pub struct Store {
    buffer_pool: Arc<Mutex<BufferPool>>,
    header: DbFileHeader,
    scheduler: Option<ScheduleHandle>,
}

impl Store {
    /// Creates a new store instance for the db found at `store_path`
    pub fn new(
        store_path: &str,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        pool_capacity: Option<usize>,
        compaction_interval: Option<u32>,
    ) -> io::Result<Self> {
        let db_folder = Path::new(store_path);
        let db_file_path = db_folder.join(DEFAULT_DB_FILE);

        initialize_db_folder(db_folder)?;

        let buffer_pool = BufferPool::new(
            pool_capacity,
            &db_file_path,
            max_keys,
            redundant_blocks,
            None,
        )?;

        let header = extract_header_from_buffer_pool(&buffer_pool)?;
        let buffer_pool = Arc::new(Mutex::new(buffer_pool));
        let scheduler = initialize_compaction_scheduler(compaction_interval, &buffer_pool);

        let store = Self {
            buffer_pool,
            header,
            scheduler,
        };

        Ok(store)
    }

    /// Sets the given key value in the store
    pub fn set(&mut self, k: &[u8], v: &[u8], ttl: Option<u64>) -> io::Result<()> {
        let expiry = match ttl {
            None => 0u64,
            Some(expiry) => get_current_timestamp() + expiry,
        };

        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool = acquire_lock!(self.buffer_pool)?;

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes =
                buffer_pool.read_at(index_offset, INDEX_ENTRY_SIZE_IN_BYTES as usize)?;
            let entry_offset = u64::from_be_bytes(slice_to_array(&kv_offset_in_bytes)?);

            if entry_offset == 0 || buffer_pool.addr_belongs_to_key(entry_offset, k)? {
                let kv = KeyValueEntry::new(k, v, expiry);
                let mut kv_bytes = kv.as_bytes();
                let prev_last_offset = buffer_pool.append(&mut kv_bytes)?;
                buffer_pool.replace(index_offset, &prev_last_offset.to_be_bytes())?;
                return Ok(());
            }

            index_block += 1;
        }

        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("CollisionSaturatedError: no free slot for key: {:?}", k),
        ))
    }

    /// Returns the value corresponding to the given key
    pub fn get(&mut self, k: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool = acquire_lock!(self.buffer_pool)?;

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes =
                buffer_pool.read_at(index_offset, INDEX_ENTRY_SIZE_IN_BYTES as usize)?;
            let entry_offset = u64::from_be_bytes(slice_to_array(&kv_offset_in_bytes)?);

            if entry_offset != 0 {
                if let Some(v) = buffer_pool.get_value(entry_offset, k)? {
                    let value = match v.is_expired {
                        true => {
                            // erase the index
                            buffer_pool.replace(index_offset, &0u64.to_be_bytes())?;
                            None
                        }
                        false => Some(v.data),
                    };
                    return Ok(value);
                }
            }

            index_block += 1;
        }

        Ok(None)
    }

    /// Deletes the key-value for the given key
    pub fn delete(&mut self, k: &[u8]) -> io::Result<()> {
        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool = acquire_lock!(self.buffer_pool)?;

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes =
                buffer_pool.read_at(index_offset, INDEX_ENTRY_SIZE_IN_BYTES as usize)?;
            let entry_offset = u64::from_be_bytes(slice_to_array(&kv_offset_in_bytes)?);

            if entry_offset != 0 && buffer_pool.addr_belongs_to_key(entry_offset, k)? {
                // erase the index
                buffer_pool.replace(index_offset, &0u64.to_be_bytes())?;

                return Ok(());
            }

            index_block += 1;
        }

        Ok(())
    }

    /// Clears the data in the file and the cache
    pub fn clear(&mut self) -> io::Result<()> {
        let mut buffer_pool = acquire_lock!(self.buffer_pool)?;
        buffer_pool.clear_file()
    }

    /// Compact the data in the file
    pub fn compact(&mut self) -> io::Result<()> {
        let mut buffer_pool = acquire_lock!(self.buffer_pool)?;
        buffer_pool.compact_file()
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Some(scheduler) = self.scheduler.take() {
            scheduler.stop();
        }
    }
}

/// Initializes the scheduler that is to run the background task of compacting the store
/// If interval (in seconds) passed is 0, No scheduler is created. The default interval is 1 hours
fn initialize_compaction_scheduler(
    interval: Option<u32>,
    buffer_pool: &Arc<Mutex<BufferPool>>,
) -> Option<ScheduleHandle> {
    let interval = match interval {
        None => 3_600u32,
        Some(v) => v,
    };

    if interval > 0 {
        let mut scheduler = Scheduler::new();
        let buffer_pool = buffer_pool.clone();

        scheduler.every(interval.seconds()).run(move || {
            let mut buffer_pool = acquire_lock!(buffer_pool).expect("get lock on buffer pool");
            buffer_pool.compact_file().expect("compact file in thread")
        });

        let handle = scheduler.watch_thread(Duration::from_millis(200));
        Some(handle)
    } else {
        None
    }
}

/// Initializes the header given the buffer bool
fn extract_header_from_buffer_pool(buffer_pool: &BufferPool) -> io::Result<DbFileHeader> {
    let mut file = acquire_lock!(buffer_pool.file)?;
    DbFileHeader::from_file(file.deref_mut())
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom};
    use std::{fs, io, thread};

    use serial_test::serial;

    use super::*;

    const STORE_PATH: &str = "db";

    /// Asserts that two lists of Result<Option<T>> are equal
    macro_rules! assert_list_eq {
        ($expected:expr, $got:expr) => {
            for (got, expected) in $got.into_iter().zip($expected) {
                assert_eq!(got.as_ref().unwrap(), expected.as_ref().unwrap());
            }
        };
    }

    #[test]
    #[serial]
    fn set_works() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);
        let received_values = get_values_for_keys(&mut store, &keys);

        let expected_values = wrap_values_in_result(&values);
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn set_with_ttl_works() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys[0..2].to_vec(), &values, None);
        insert_test_data(&mut store, &keys[2..].to_vec(), &values, Some(1)); // 1 second ttl

        // wait for expiry and some more just to be safe
        thread::sleep(Duration::from_secs(2));

        let received_values = get_values_for_keys(&mut store, &keys);
        let mut expected_values = wrap_values_in_result(&values[..2]);
        for _ in 2..keys.len() {
            expected_values.push(Ok(None));
        }

        assert_list_eq!(&expected_values, &received_values);
        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn set_can_update() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();
        let unchanged_values = values[2..].to_vec();
        let updated_keys = keys[0..2].to_vec();
        let updated_values: Vec<Vec<u8>> = values[0..2]
            .iter()
            .map(|v| v.iter().chain(b"bear").map(|v| v.to_owned()).collect())
            .collect();

        insert_test_data(&mut store, &keys, &values, None);
        insert_test_data(&mut store, &updated_keys, &updated_values, None);
        let received_values = get_values_for_keys(&mut store, &keys);
        let received_unchanged_values = &received_values[2..];
        let received_updated_values = &received_values[0..2];

        // unchanged
        let expected_unchanged_values = wrap_values_in_result(&unchanged_values);
        let expected_updated_values = wrap_values_in_result(&updated_values);

        assert_list_eq!(&expected_unchanged_values, &received_unchanged_values);
        assert_list_eq!(&expected_updated_values, &received_updated_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn delete_works() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values, None);
        delete_keys(&mut store, &keys_to_delete);

        let received_values = get_values_for_keys(&mut store, &keys);
        let mut expected_values = wrap_values_in_result(&values[..2]);
        for _ in 0..keys_to_delete.len() {
            expected_values.push(Ok(None));
        }
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn clear_works() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);
        store.clear().expect("store cleared");

        let received_values = get_values_for_keys(&mut store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys.iter().map(|_| Ok(None)).collect();
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn persists_to_file() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store
            .clear()
            .expect("store failed to get cleared for some reason");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");

        let received_values = get_values_for_keys(&mut store, &keys);
        let expected_values = wrap_values_in_result(&values);
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn persists_to_file_after_delete() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values, None);
        delete_keys(&mut store, &keys_to_delete);

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");

        let received_values = get_values_for_keys(&mut store, &keys);
        let mut expected_values = wrap_values_in_result(&values[..2]);
        for _ in 0..keys_to_delete.len() {
            expected_values.push(Ok(None));
        }
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn persists_to_file_after_clear() {
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);
        store.clear().expect("store failed to clear");

        // Open new store instance
        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");

        let received_values = get_values_for_keys(&mut store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys.iter().map(|_| Ok(None)).collect();

        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn compact_removes_deleted_and_expired_filed() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        let mut store = Store::new(STORE_PATH, None, None, None, Some(0)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(
            &mut store,
            &keys[0..2].to_vec(),
            &values[0..2].to_vec(),
            Some(1),
        );
        insert_test_data(&mut store, &keys[2..].to_vec(), &values[2..].to_vec(), None);
        delete_keys(&mut store, &keys[2..3].to_vec());

        let buffer_pool = acquire_lock!(store.buffer_pool).expect("acquire lock on buffer pool");
        let db_file_path = buffer_pool.file_path.to_str().unwrap().to_owned();
        drop(buffer_pool);

        // wait for some keys to expire
        thread::sleep(Duration::from_secs(2));

        let original_file_size = get_file_size(&db_file_path);

        store.compact().expect("compact store");

        let final_file_size = get_file_size(&db_file_path);
        let expected_file_size_reduction = keys[0..3]
            .iter()
            .zip(&values[0..3])
            .map(|(k, v)| KeyValueEntry::new(k, v, 0).as_bytes().len() as u64)
            .reduce(|accum, v| accum + v)
            .unwrap();

        assert_eq!(
            original_file_size - final_file_size,
            expected_file_size_reduction
        );

        // And the store is still acting as before
        let received_values = get_values_for_keys(&mut store, &keys);
        let received_unchanged_values = &received_values[3..];
        let received_removed_values = &received_values[0..3];

        // unchanged
        let expected_unchanged_values = wrap_values_in_result(&values[3..]);
        let expected_expired_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys[0..3].iter().map(|_| Ok(None)).collect();

        assert_list_eq!(&expected_unchanged_values, &received_unchanged_values);
        assert_list_eq!(&expected_expired_values, &received_removed_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn background_task_compacts() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        // set the compaction interval to 1 second
        let mut store = Store::new(STORE_PATH, None, None, None, Some(1)).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(
            &mut store,
            &keys[0..2].to_vec(),
            &values[0..2].to_vec(),
            Some(1),
        );
        insert_test_data(&mut store, &keys[2..].to_vec(), &values[2..].to_vec(), None);
        delete_keys(&mut store, &keys[2..3].to_vec());

        let buffer_pool = acquire_lock!(store.buffer_pool).expect("acquire lock on buffer pool");
        let db_file_path = buffer_pool.file_path.to_str().unwrap().to_owned();
        drop(buffer_pool);

        let original_file_size = get_file_size(&db_file_path);

        // wait for some keys to expire
        thread::sleep(Duration::from_secs(2));

        // store.compact().expect("compact store");

        let final_file_size = get_file_size(&db_file_path);
        let expected_file_size_reduction = keys[0..3]
            .iter()
            .zip(&values[0..3])
            .map(|(k, v)| KeyValueEntry::new(k, v, 0).as_bytes().len() as u64)
            .reduce(|accum, v| accum + v)
            .unwrap();

        assert_eq!(
            original_file_size - final_file_size,
            expected_file_size_reduction
        );

        // And the store is still acting as before
        let received_values = get_values_for_keys(&mut store, &keys);
        let received_unchanged_values = &received_values[3..];
        let received_removed_values = &received_values[0..3];

        // unchanged
        let expected_unchanged_values = wrap_values_in_result(&values[3..]);
        let expected_expired_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys[0..3].iter().map(|_| Ok(None)).collect();

        assert_list_eq!(&expected_unchanged_values, &received_unchanged_values);
        assert_list_eq!(&expected_expired_values, &received_removed_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    /// Deletes the given keys in the store
    fn delete_keys(store: &mut Store, keys_to_delete: &Vec<Vec<u8>>) {
        for k in keys_to_delete {
            store.delete(k).expect(&format!("delete key {:?}", k));
        }
    }

    /// Returns the actual file size of the file at the given path
    fn get_file_size(file_path: &str) -> u64 {
        let mut file = OpenOptions::new()
            .read(true)
            .open(file_path)
            .expect(&format!("open file {}", file_path));
        file.seek(SeekFrom::End(0)).expect("get file size")
    }

    /// Gets a vector of responses from the store when store.get is called
    /// for each key passed in keys
    fn get_values_for_keys(
        store: &mut Store,
        keys: &Vec<Vec<u8>>,
    ) -> Vec<io::Result<Option<Vec<u8>>>> {
        let mut received_values = Vec::with_capacity(keys.len());

        for k in keys {
            let _ = &received_values.push(store.get(k));
        }

        received_values
    }

    /// Inserts test data into the store
    fn insert_test_data(
        store: &mut Store,
        keys: &Vec<Vec<u8>>,
        values: &Vec<Vec<u8>>,
        ttl: Option<u64>,
    ) {
        for (k, v) in keys.iter().zip(values) {
            store
                .set(k, v, ttl)
                .expect(&format!("set key {:?}, value {:?}", k, v));
        }
    }

    /// Gets keys for testing
    fn get_keys() -> Vec<Vec<u8>> {
        ["hey", "hi", "yoo-hoo", "bonjour", "oloota", "orirota"]
            .into_iter()
            .map(|v| v.to_string().into_bytes())
            .collect()
    }

    /// Gets values for testing
    fn get_values() -> Vec<Vec<u8>> {
        [
            "English",
            "English",
            "Slang",
            "French",
            "Runyoro",
            "Runyakole",
        ]
        .into_iter()
        .map(|v| v.to_string().into_bytes())
        .collect()
    }

    /// Wraps values in Result<Option<T>>
    fn wrap_values_in_result(values: &[Vec<u8>]) -> Vec<io::Result<Option<Vec<u8>>>> {
        values.iter().map(|v| Ok(Some(v.clone()))).collect()
    }
}
