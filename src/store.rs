use std::fmt::{Debug, Display, Formatter};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::{io, thread};

use clokwerk::{ScheduleHandle, Scheduler, TimeUnits};

use crate::internal::{
    acquire_lock, get_current_timestamp, initialize_db_folder, slice_to_array, BufferPool,
    DbFileHeader, Header, InvertedIndex, KeyValueEntry, ValueEntry,
};

const DEFAULT_DB_FILE: &str = "dump.scdb";
const DEFAULT_SEARCH_INDEX_FILE: &str = "index.iscdb";
const ZERO_U64_BYTES: [u8; 8] = 0u64.to_be_bytes();

/// A key-value store that persists key-value pairs to disk
///
/// Store behaves like a HashMap that saves keys and value as byte arrays
/// on disk. It allows for specifying how long each key-value pair should be
/// kept for i.e. the time-to-live in seconds. If None is provided, they last indefinitely.
///
/// # Configuration
///
/// The Store has a number of configurations that are passed into the new() method
///
/// - `store_path` - required: The path to a directory where scdb should store its data
/// - `max_keys` - default: 1 million: The maximum number of key-value pairs to store in store
/// - `redundant_blocks` - default: 1: The store has an index to hold all the keys. This index is split
///                                     into a fixed number of blocks basing on the virtual memory page size
///                                     and the total number of keys to be held i.e. `max_keys`.
///                                     Sometimes, there may be hash collision errors as the store's
///                                     current stored keys approach `max_keys`. The closer it gets, the
///                                     more it becomes likely see those errors. Adding redundant blocks
///                                     helps mitigate this. Just be careful to not add too many (i.e. more than 2)
///                                     since the higher the number of these blocks, the slower the store becomes.
/// - `pool_capacity` - default: 5: The number of buffers to hold in memory as cache's for the store. Each buffer
///                                 has the size equal to the virtual memory's page size, usually 4096 bytes.
///                                 Increasing this number will speed this store up but of course, the machine
///                                 has a limited RAM. When this number increases to a value that clogs the RAM, performance
///                                 suddenly degrades, and keeps getting worse from there on.
/// - `compaction_interval` - default 3600s (1 hour): The interval at which the store is compacted to remove dangling
///                                                     keys. Dangling keys result from either getting expired or being deleted.
///                                                     When a `delete` operation is done, the actual key-value pair
///                                                     is just marked as `deleted` but is not removed.
///                                                     Something similar happens when a key-value is updated.
///                                                     A new key-value pair is created and the old one is left unindexed.
///                                                     Compaction is important because it reclaims this space and reduces the size
///                                                     of the database file.
/// - `max_index_key_len` - default 3: The maximum number of characters in each key in the search inverted index
///                                             The inverted index is used for full-text search of keys to get all key-values
///                                             whose keys start with a given byte array.
///
/// # Examples
///
/// ```rust
/// # use std::io;
///
/// # fn main() -> io::Result<()> {
///     // Create the store. You can configure its `max_keys`, `redundant_blocks` etc.
///     // The defaults are usable though.
///     // One very important config is `max_keys`.
///     // With it, you can limit the store size to a number of keys.
///     // By default, the limit is 1 million keys
///     let mut store = scdb::Store::new("db", // `store_path`
///                             Some(1000), // `max_keys`
///                             Some(1), // `redundant_blocks`
///                             Some(10), // `pool_capacity`
///                             Some(1800),// `compaction_interval`
///                             Some(3))?; // `max_index_key_len`
///     let key = b"foo";
///     let value = b"bar";
///
///     // Insert key-value pair into the store with no time-to-live
///     store.set(&key[..], &value[..], None)?;
///     # assert_eq!(store.get(&key[..])?, Some(value.to_vec()));
///
///     // Or insert it with an optional time-to-live (ttl)
///     // It will disappear from the store after `ttl` seconds
///     store.set(&key[..], &value[..], Some(1))?;
///     # assert_eq!(store.get(&key[..])?, Some(value.to_vec()));
///
///     // Getting the values by passing the key in bytes to store.get
///     let value_in_store = store.get(&key[..])?;
///     assert_eq!(value_in_store, Some(value.to_vec()));
///
///     // Updating the values is just like inserting them. Any key-value already in the store will
///     // be overwritten
///     store.set(&key[..], &value[..], None)?;
///
///     // Delete the key-value pair by supplying the key as an argument to store.delete
///     store.delete(&key[..])?;
///     assert_eq!(store.get(&key[..])?, None);
///
///     // Deleting all key-value pairs to start afresh, use store.clear()
///     # store.set(&key[..], &value[..], None)?;
///     store.clear()?;
///     # assert_eq!(store.get(&key[..])?, None);
///
///     # Ok(())
/// # }
/// ```
pub struct Store {
    buffer_pool: Arc<Mutex<BufferPool>>,
    header: DbFileHeader,
    scheduler: Option<ScheduleHandle>,
    search_index: Arc<Mutex<InvertedIndex>>,
}

impl Store {
    /// Creates a new store instance for the db found at `store_path`
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] if it can't write to the `store_path` say due to permissions errors
    ///
    /// # Examples
    ///
    /// ```rust
    /// use scdb::Store;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let store = Store::new("db", None, None, None, None, None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        store_path: &str,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        pool_capacity: Option<usize>,
        compaction_interval: Option<u32>,
        max_index_key_len: Option<u32>,
    ) -> io::Result<Self> {
        let db_folder = Path::new(store_path);
        let db_file_path = db_folder.join(DEFAULT_DB_FILE);
        let search_idx_file_path = db_folder.join(DEFAULT_SEARCH_INDEX_FILE);

        initialize_db_folder(db_folder)?;

        let mut buffer_pool = BufferPool::new(
            pool_capacity,
            &db_file_path,
            max_keys,
            redundant_blocks,
            None,
        )?;

        let search_index = InvertedIndex::new(
            &search_idx_file_path,
            max_index_key_len,
            max_keys,
            redundant_blocks,
        )?;

        let header = extract_header_from_buffer_pool(&mut buffer_pool)?;
        let buffer_pool = Arc::new(Mutex::new(buffer_pool));
        let search_index = Arc::new(Mutex::new(search_index));
        let scheduler = initialize_scheduler(compaction_interval, &buffer_pool, &search_index);

        let store = Self {
            buffer_pool,
            header,
            scheduler,
            search_index,
        };

        Ok(store)
    }

    /// Sets the given key value in the store
    ///
    /// This is used to insert or update any key-value pair in the store
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case the keys are maxed out i.e the store
    /// has reached its capacity in terms of number of unexpired key-value keys it can hold
    /// It may also fail with 'collision saturated' errors when the number of unexpired keys in the store
    /// is almost reaching `max_keys`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut  store = Store::new("db", None, None, None, None, None)?;
    /// // set a key-value pair that never expires
    /// store.set(&b"foo"[..], &b"bar"[..], None)?;
    /// # assert_eq!(store.get(&b"foo"[..])?, Some(b"bar".to_vec()));
    ///
    /// // set a key-value pair that expires after 5 seconds
    /// store.set(&b"foo2"[..], &b"bar2"[..], Some(5))?;
    /// # assert_eq!(store.get(&b"foo2"[..])?, Some(b"bar2".to_vec()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn set(&mut self, k: &[u8], v: &[u8], ttl: Option<u64>) -> io::Result<()> {
        let expiry = match ttl {
            None => 0u64,
            Some(expiry) => get_current_timestamp() + expiry,
        };

        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes = buffer_pool.read_index(index_offset)?;

            if kv_offset_in_bytes == ZERO_U64_BYTES
                || buffer_pool.addr_belongs_to_key(&kv_offset_in_bytes, k)?
            {
                let kv = KeyValueEntry::new(k, v, expiry);
                let mut kv_bytes = kv.as_bytes();
                let prev_last_offset = buffer_pool.append(&mut kv_bytes)?;
                let kv_address = prev_last_offset.to_be_bytes();
                buffer_pool.update_index(index_offset, &kv_address)?;

                // Update the search index
                let mut search_index: MutexGuard<'_, InvertedIndex> =
                    acquire_lock!(self.search_index)?;
                search_index.add(k, prev_last_offset, expiry)?;

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
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case it cannot access the database file say if it deleted
    /// or due to permissions errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut  store = Store::new("db", None, None, None, None, None)?;
    /// # store.clear()?;
    /// # store.set(&b"foo"[..], &b"bar"[..], None)?;
    /// // if (b"foo", b"bar") exists,
    /// // the value returned will be Some(b"bar")
    /// let value = store.get(&b"foo"[..])?;
    /// assert_eq!(value, Some(b"bar".to_vec()));
    ///
    /// // It returns None for non-existent keys or expired keys
    /// assert_eq!(store.get(&b"foo2"[..])?, None);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&mut self, k: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes = buffer_pool.read_index(index_offset)?;

            if kv_offset_in_bytes != ZERO_U64_BYTES {
                let entry_offset = u64::from_be_bytes(slice_to_array(&kv_offset_in_bytes)?);

                if let Some(v) = buffer_pool.get_value(entry_offset, k)? {
                    return if v.is_stale {
                        Ok(None)
                    } else {
                        Ok(Some(v.data))
                    };
                }
            }

            index_block += 1;
        }

        Ok(None)
    }

    /// Deletes the key-value for the given key
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case it cannot access the database file say if it deleted
    /// or due to permissions errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut  store = Store::new("db", None, None, None, None, None)?;
    /// # store.clear()?;
    /// # store.set(&b"foo"[..], &b"bar"[..], None)?;
    /// // if (b"foo", b"bar") exists
    /// assert_eq!(store.get(&b"foo"[..])?, Some(b"bar".to_vec()));
    ///
    /// // deleting it removes it from the store
    /// store.delete(&b"foo"[..])?;
    /// assert_eq!(store.get(&b"foo"[..])?, None);
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&mut self, k: &[u8]) -> io::Result<()> {
        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(k);
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;

        // Update the search index in a separate thread.
        let search_index = self.search_index.clone();
        let k_copy = k.to_vec();
        let search_handle = thread::spawn(move || {
            let mut search_index: MutexGuard<'_, InvertedIndex> = acquire_lock!(search_index)?;
            search_index.remove(&k_copy)
        });

        // delete from the scdb file
        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let kv_offset_in_bytes = buffer_pool.read_index(index_offset)?;

            if kv_offset_in_bytes != ZERO_U64_BYTES {
                let entry_offset = u64::from_be_bytes(slice_to_array(&kv_offset_in_bytes)?);

                if let Some(()) = buffer_pool.try_delete_kv_entry(entry_offset, k)? {
                    return Ok(());
                }
            }

            index_block += 1;
        }

        search_handle.join().unwrap()
    }

    /// Clears all data in the store
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case it cannot access the database file say if it deleted
    /// or due to permissions errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut  store = Store::new("db", None, None, None, None, None)?;
    /// # store.clear()?;
    /// # store.set(&b"foo"[..], &b"bar"[..], None)?;
    /// # store.set(&b"foo2"[..], &b"bar2"[..], None)?;
    /// // if (b"foo", b"bar"), (b"foo2", b"bar2") exist
    /// assert_eq!(store.get(&b"foo"[..])?, Some(b"bar".to_vec()));
    /// assert_eq!(store.get(&b"foo2"[..])?, Some(b"bar2".to_vec()));
    /// // clear removes everything from the store
    /// store.clear()?;
    /// assert_eq!(store.get(&b"foo"[..])?, None);
    /// assert_eq!(store.get(&b"foo2"[..])?, None);
    /// # Ok(())
    /// # }
    /// ```
    pub fn clear(&mut self) -> io::Result<()> {
        // Clear the search index in a separate thread
        let search_index = self.search_index.clone();
        let search_handle = thread::spawn(move || {
            let mut search_index: MutexGuard<'_, InvertedIndex> = acquire_lock!(search_index)?;
            search_index.clear()
        });

        // Clear the scdb file
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;
        buffer_pool.clear_file()?;

        search_handle.join().unwrap()
    }

    /// Manually removes dangling key-value pairs in the database file
    ///
    /// Dangling keys result from either getting expired or being deleted.
    /// When a `delete` operation is done, the actual key-value pair
    /// is just marked as `deleted` but is not removed.
    ///                                                     
    /// Something similar happens when a key-value is updated.
    /// A new key-value pair is created and the old one is left un-indexed.
    /// Compaction is important because it reclaims this space and reduces the size
    /// of the database file.
    ///
    /// This is done automatically for you at the set `compaction_interval` but you
    /// may wish to do it manually for some reason.
    ///
    /// This is a very expensive operation so use it sparingly.
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case it cannot access the database file say if it deleted
    /// or due to permissions errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut store = Store::new("db", None, None, None, None, None)?;
    /// store.compact()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn compact(&mut self) -> io::Result<()> {
        // Compact the scdb file
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;
        let mut search_index: MutexGuard<'_, InvertedIndex> = acquire_lock!(self.search_index)?;
        // Since compacting the db file disorganizes the addresses, we will rebuild
        // the index every time compaction of db is done.
        buffer_pool.compact_file(&mut search_index)
    }

    /// Searches for unexpired keys that start with the given search term
    ///
    /// It skips the first `skip` (default: 0) number of results and returns not more than
    /// `limit` (default: 0) number of items. This is to avoid using up more memory than can be handled by the
    /// host machine.
    ///
    /// If `limit` is 0, all items are returned since it would make no sense for someone to search
    /// for zero items.
    ///
    /// returns a list of tuples of key-value
    ///
    /// # Errors
    ///
    /// It may fail with [std::io::Error] in case it cannot access the database file say if it deleted
    /// or due to permissions errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use scdb::Store;
    /// #
    /// # fn main() -> std::io::Result<()> {
    /// # let mut  store = Store::new("db", None, None, None, None, None)?;
    /// # store.clear()?;   
    /// // imagine the store has the following key value pairs
    /// let data = vec![
    ///     (&b"hi"[..], &b"ooliyo"[..]),
    ///     (&b"high"[..], &b"haiguru"[..]),
    ///     (&b"hind"[..], &b"enyuma"[..]),
    ///     (&b"hill"[..], &b"akasozi"[..]),
    ///     (&b"him"[..], &b"ogwo"[..]),
    /// ];
    /// # let mut expected: Vec<(Vec<u8>, Vec<u8>)> = vec![];
    /// # for (k, v) in data {
    /// #    store.set(k, v, None)?;
    /// #    expected.push((k.to_vec(), v.to_vec()))
    /// # }
    /// // search for key-values where the keys start with 'hi'
    /// let key_values = store.search(&b"hi"[..], 0, 0)?;
    /// assert_eq!(key_values, expected);
    ///
    /// // Or just return a few of them, say last three
    /// let key_values = store.search(&b"hi"[..], 2, 3)?;
    /// assert_eq!(key_values, expected[2..]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn search(
        &mut self,
        term: &[u8],
        skip: u64,
        limit: u64,
    ) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut search_index = acquire_lock!(self.search_index)?;
        let offsets = search_index.search(term, skip, limit)?;
        let mut buffer_pool: MutexGuard<'_, BufferPool> = acquire_lock!(self.buffer_pool)?;
        buffer_pool.get_many_key_values(&offsets)
    }
}

impl Debug for Store {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Store {{ buffer_pool: {:?}, header: {}}}",
            self.buffer_pool, self.header
        )
    }
}

impl Display for Store {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
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
/// If interval (in seconds) passed is 0, No scheduler is created. The default interval is 1 hour
fn initialize_scheduler(
    interval: Option<u32>,
    buffer_pool: &Arc<Mutex<BufferPool>>,
    search_index: &Arc<Mutex<InvertedIndex>>,
) -> Option<ScheduleHandle> {
    let interval = interval.unwrap_or(3_600u32);

    if interval > 0 {
        let mut scheduler = Scheduler::new();
        let buffer_pool = buffer_pool.clone();
        let search_index = search_index.clone();

        scheduler.every(interval.seconds()).run(move || {
            let mut buffer_pool = acquire_lock!(buffer_pool).expect("get lock on buffer pool");
            // Since compacting the db file disorganizes the addresses, we will rebuild
            // the index every time compaction of db is done
            let mut search_index: MutexGuard<'_, InvertedIndex> =
                acquire_lock!(search_index).expect("get lock on search index");
            buffer_pool
                .compact_file(&mut search_index)
                .expect("compact db file in thread");
        });

        let handle = scheduler.watch_thread(Duration::from_millis(200));
        Some(handle)
    } else {
        None
    }
}

/// Initializes the header given the buffer bool
fn extract_header_from_buffer_pool(buffer_pool: &mut BufferPool) -> io::Result<DbFileHeader> {
    DbFileHeader::from_file(&mut buffer_pool.file)
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

    /// Converts a string slice into bytes
    macro_rules! str_to_bytes {
        ($v:expr) => {
            $v.to_string().into_bytes()
        };
    }

    /// Converts an array of strings into a vector of byte arrays
    macro_rules! to_byte_arrays_vector {
        ($data:expr) => {
            $data.map(|v| str_to_bytes!(v)).to_vec()
        };
    }

    #[test]
    #[serial]
    fn set_works() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
    fn search_works() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "fore", "bar", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "span", "port", "nyoro", "dan"]);

        insert_test_data(&mut store, &keys, &values, None);
        let test_data = [
            ("f", vec![("foo", "eng"), ("fore", "span")]),
            ("fo", vec![("foo", "eng"), ("fore", "span")]),
            ("foo", vec![("foo", "eng")]),
            ("for", vec![("fore", "span")]),
            ("b", vec![("bar", "port"), ("band", "nyoro")]),
            ("ba", vec![("bar", "port"), ("band", "nyoro")]),
            ("bar", vec![("bar", "port")]),
            ("ban", vec![("band", "nyoro")]),
            ("band", vec![("band", "nyoro")]),
            ("p", vec![("pig", "dan")]),
            ("pi", vec![("pig", "dan")]),
            ("pig", vec![("pig", "dan")]),
            ("pigg", vec![]),
            ("food", vec![]),
            ("bandana", vec![]),
            ("bare", vec![]),
        ];

        for (term, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn search_works_after_expire() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "bar", "fore", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "port", "span", "nyoro", "dan"]);

        insert_test_data(&mut store, &keys.to_vec(), &values.to_vec(), Some(1));
        insert_test_data(&mut store, &keys[2..].to_vec(), &values[2..].to_vec(), None);

        // wait for expiry and some more just to be safe
        thread::sleep(Duration::from_secs(2));

        // expired items are ignored
        let test_data = [
            ("f", vec![("fore", "span")]),
            ("fo", vec![("fore", "span")]),
            ("foo", vec![]),
            ("for", vec![("fore", "span")]),
            ("b", vec![("band", "nyoro")]),
            ("ba", vec![("band", "nyoro")]),
            ("bar", vec![]),
            ("ban", vec![("band", "nyoro")]),
            ("band", vec![("band", "nyoro")]),
            ("p", vec![("pig", "dan")]),
            ("pi", vec![("pig", "dan")]),
            ("pig", vec![("pig", "dan")]),
            ("pigg", vec![]),
            ("food", vec![]),
            ("bandana", vec![]),
            ("bare", vec![]),
        ];

        for (term, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn search_works_after_delete() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "fore", "bar", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "span", "port", "nyoro", "dan"]);

        insert_test_data(&mut store, &keys, &values, None);
        delete_keys(&mut store, &to_byte_arrays_vector!(["foo", "bar", "band"]));
        let test_data = [
            ("f", vec![("fore", "span")]),
            ("fo", vec![("fore", "span")]),
            ("foo", vec![]),
            ("for", vec![("fore", "span")]),
            ("b", vec![]),
            ("ba", vec![]),
            ("bar", vec![]),
            ("ban", vec![]),
            ("band", vec![]),
            ("p", vec![("pig", "dan")]),
            ("pi", vec![("pig", "dan")]),
            ("pig", vec![("pig", "dan")]),
            ("pigg", vec![]),
            ("food", vec![]),
            ("bandana", vec![]),
            ("bare", vec![]),
        ];

        for (term, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn search_works_after_clear() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "fore", "bar", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "span", "port", "nyoro", "dan"]);

        insert_test_data(&mut store, &keys, &values, None);
        let test_data = [
            "f", "fo", "foo", "for", "b", "ba", "bar", "ban", "band", "p", "pi", "pig", "pigg",
            "food", "bandana", "bare",
        ];
        store.clear().expect("store cleared");
        let expected: Vec<(Vec<u8>, Vec<u8>)> = vec![];

        for term in test_data {
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn paginated_search_works() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "fore", "food", "bar", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "span", "lug", "port", "nyoro", "dan"]);

        insert_test_data(&mut store, &keys, &values, None);
        let test_data = [
            (
                "fo",
                0,
                0,
                vec![("foo", "eng"), ("fore", "span"), ("food", "lug")],
            ),
            (
                "fo",
                0,
                8,
                vec![("foo", "eng"), ("fore", "span"), ("food", "lug")],
            ),
            ("fo", 1, 8, vec![("fore", "span"), ("food", "lug")]),
            ("fo", 1, 0, vec![("fore", "span"), ("food", "lug")]),
            ("fo", 0, 2, vec![("foo", "eng"), ("fore", "span")]),
            ("fo", 1, 2, vec![("fore", "span"), ("food", "lug")]),
            ("fo", 0, 1, vec![("foo", "eng")]),
            ("fo", 2, 1, vec![("food", "lug")]),
            ("fo", 1, 1, vec![("fore", "span")]),
        ];

        for (term, skip, limit, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            let got = store
                .search(&str_to_bytes!(term), skip, limit)
                .expect(&format!(
                    "search for {}, skip: {}, limit: {}",
                    term, skip, limit
                ));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn persists_to_file() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store
            .clear()
            .expect("store failed to get cleared for some reason");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);

        // Open new store instance
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");

        let received_values = get_values_for_keys(&mut store, &keys);
        let expected_values = wrap_values_in_result(&values);
        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn persists_to_file_after_delete() {
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        let keys_to_delete = keys[2..].to_vec();

        insert_test_data(&mut store, &keys, &values, None);
        delete_keys(&mut store, &keys_to_delete);

        // Open new store instance
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");

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
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = get_keys();
        let values = get_values();

        insert_test_data(&mut store, &keys, &values, None);
        store.clear().expect("store failed to clear");

        // Open new store instance
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");

        let received_values = get_values_for_keys(&mut store, &keys);
        let expected_values: Vec<io::Result<Option<Vec<u8>>>> =
            keys.iter().map(|_| Ok(None)).collect();

        assert_list_eq!(&expected_values, &received_values);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn compact_removes_deleted_and_expired_from_db_file() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
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
    fn compact_removes_expired_from_search_index_file() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(0), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "bar", "fore", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "port", "span", "nyoro", "dan"]);

        insert_test_data(
            &mut store,
            &keys[0..2].to_vec(),
            &values[0..2].to_vec(),
            Some(1),
        );
        insert_test_data(&mut store, &keys[2..].to_vec(), &values[2..].to_vec(), None);

        let search_index = acquire_lock!(store.search_index).expect("acquire lock on search index");
        let search_index_file_path = search_index.file_path.to_str().unwrap().to_owned();
        drop(search_index);

        // wait for some keys to expire
        thread::sleep(Duration::from_secs(2));

        let original_file_size = get_file_size(&search_index_file_path);

        store.compact().expect("compact store");

        let final_file_size = get_file_size(&search_index_file_path);
        let expected_file_size_reduction = 282u64;

        assert_eq!(
            original_file_size - final_file_size,
            expected_file_size_reduction
        );

        // And the search is still acting as before, with the expired not showing up
        // expired items are ignored
        let test_data = [
            ("f", vec![("fore", "span")]),
            ("fo", vec![("fore", "span")]),
            ("foo", vec![]),
            ("for", vec![("fore", "span")]),
            ("b", vec![("band", "nyoro")]),
            ("ba", vec![("band", "nyoro")]),
            ("bar", vec![]),
            ("ban", vec![("band", "nyoro")]),
            ("band", vec![("band", "nyoro")]),
            ("p", vec![("pig", "dan")]),
            ("pi", vec![("pig", "dan")]),
            ("pig", vec![("pig", "dan")]),
            ("pigg", vec![]),
            ("food", vec![]),
            ("bandana", vec![]),
            ("bare", vec![]),
        ];

        for (term, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            // Compaction of db file moves the addresses around, therefore it must also update the inverted db!
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn background_task_compacts_db_file() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        // set the compaction interval to 1 second
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(1), None).expect("create store");
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
        thread::sleep(Duration::from_secs(4));

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

        // ensure background tasks stop running
        drop(store);

        fs::remove_dir_all(STORE_PATH).expect("delete store folder");
    }

    #[test]
    #[serial]
    fn background_task_compacts_search_index_file() {
        // pre-clean up for the right results
        fs::remove_dir_all(STORE_PATH).ok();

        // set the compaction interval to 1 second
        let mut store =
            Store::new(STORE_PATH, None, None, None, Some(1), None).expect("create store");
        store.clear().expect("store failed to clear");
        let keys = to_byte_arrays_vector!(["foo", "bar", "fore", "band", "pig"]);
        let values = to_byte_arrays_vector!(["eng", "port", "span", "nyoro", "dan"]);

        insert_test_data(
            &mut store,
            &keys[0..2].to_vec(),
            &values[0..2].to_vec(),
            Some(1),
        );
        insert_test_data(&mut store, &keys[2..].to_vec(), &values[2..].to_vec(), None);

        let search_index = acquire_lock!(store.search_index).expect("acquire lock on search index");
        let search_index_file_path = search_index.file_path.to_str().unwrap().to_owned();
        drop(search_index);

        let original_file_size = get_file_size(&search_index_file_path);

        // wait for some keys to expire
        thread::sleep(Duration::from_secs(4));

        let final_file_size = get_file_size(&search_index_file_path);
        let expected_file_size_reduction = 282u64;

        assert_eq!(
            original_file_size - final_file_size,
            expected_file_size_reduction
        );

        // And the search is still acting as before, with the expired not showing up
        // expired items are ignored
        let test_data = [
            ("f", vec![("fore", "span")]),
            ("fo", vec![("fore", "span")]),
            ("foo", vec![]),
            ("for", vec![("fore", "span")]),
            ("b", vec![("band", "nyoro")]),
            ("ba", vec![("band", "nyoro")]),
            ("bar", vec![]),
            ("ban", vec![("band", "nyoro")]),
            ("band", vec![("band", "nyoro")]),
            ("p", vec![("pig", "dan")]),
            ("pi", vec![("pig", "dan")]),
            ("pig", vec![("pig", "dan")]),
            ("pigg", vec![]),
            ("food", vec![]),
            ("bandana", vec![]),
            ("bare", vec![]),
        ];

        for (term, expected) in test_data {
            let expected: Vec<(Vec<u8>, Vec<u8>)> = expected
                .into_iter()
                .map(|(k, v)| (str_to_bytes!(k), str_to_bytes!(v)))
                .collect();
            let got = store
                .search(&str_to_bytes!(term), 0, 0)
                .expect(&format!("search for {}", term));
            assert_eq!(&expected, &got);
        }

        // ensure background tasks stop running
        drop(store);

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
        to_byte_arrays_vector!(["hey", "hi", "yoo-hoo", "bonjour", "oloota", "orirota"])
    }

    /// Gets values for testing
    fn get_values() -> Vec<Vec<u8>> {
        to_byte_arrays_vector!([
            "English",
            "English",
            "Slang",
            "French",
            "Runyoro",
            "Runyakole",
        ])
    }

    /// Wraps values in Result<Option<T>>
    fn wrap_values_in_result(values: &[Vec<u8>]) -> Vec<io::Result<Option<Vec<u8>>>> {
        values.iter().map(|v| Ok(Some(v.clone()))).collect()
    }
}
