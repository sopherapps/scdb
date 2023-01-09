use crate::internal::buffers::buffer::{Buffer, Value};
use crate::internal::entries::headers::shared::{HEADER_SIZE_IN_BYTES, INDEX_ENTRY_SIZE_IN_BYTES};
use crate::internal::entries::index::Index;
use crate::internal::entries::values::key_value::OFFSET_FOR_KEY_IN_KV_ARRAY;
use crate::internal::entries::values::shared::ValueEntry;
use crate::internal::macros::validate_bounds;
use crate::internal::utils::{get_vm_page_size, TRUE_AS_BYTE};
use crate::internal::{acquire_lock, slice_to_array, DbFileHeader, Header, KeyValueEntry};
use std::cmp::{max, min};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

const DEFAULT_POOL_CAPACITY: usize = 5;

/// A pool of Buffers.
///
/// It is possible to have more than one buffer with the same address in a kind of overlap
/// In order to avoid corruption, we always update the last buffer that has a given address
/// since buffers are in FIFO queue. When retrieving a value, we also use the last buffer
/// that has a given address
#[derive(Debug)]
pub(crate) struct BufferPool {
    kv_capacity: usize,
    index_capacity: usize,
    buffer_size: usize,
    key_values_start_point: u64,
    max_keys: Option<u64>,
    redundant_blocks: Option<u16>,
    kv_buffers: VecDeque<Buffer>,
    index_buffers: BTreeMap<u64, Buffer>,
    pub(crate) file: File,
    pub(crate) file_path: PathBuf,
    pub(crate) file_size: u64,
}

impl BufferPool {
    /// Creates a new BufferPool with the given `capacity` number of Buffers and
    /// for the file at the given path (creating it if necessary)
    pub(crate) fn new(
        capacity: Option<usize>,
        file_path: &Path,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        buffer_size: Option<usize>,
    ) -> io::Result<Self> {
        let buffer_size = buffer_size.unwrap_or(get_vm_page_size() as usize);
        let capacity = capacity.unwrap_or(DEFAULT_POOL_CAPACITY);

        let should_create_new = !file_path.exists();
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(should_create_new)
            .open(file_path)?;

        let header = if should_create_new {
            let header = DbFileHeader::new(max_keys, redundant_blocks, Some(buffer_size as u32));
            initialize_db_file(&mut file, &header)?;
            header
        } else {
            DbFileHeader::from_file(&mut file)?
        };

        let file_size = file.seek(SeekFrom::End(0))?;

        let index_capacity = get_index_capacity(header.number_of_index_blocks as usize, capacity);
        let kv_capacity = capacity - index_capacity;

        let v = Self {
            kv_capacity,
            index_capacity,
            buffer_size,
            max_keys,
            redundant_blocks,
            key_values_start_point: header.key_values_start_point,
            kv_buffers: VecDeque::with_capacity(kv_capacity),
            index_buffers: Default::default(),
            file,
            file_size,
            file_path: file_path.into(),
        };

        Ok(v)
    }

    /// Appends a given data array to the file attached to this buffer pool
    /// It returns the address where the data was appended
    pub(crate) fn append(&mut self, data: &mut Vec<u8>) -> io::Result<u64> {
        // loop in reverse, starting at the back
        // since the latest kv_buffers are the ones updated when new changes occur
        for buf in self.kv_buffers.iter_mut().rev() {
            if buf.can_append(self.file_size) {
                let addr = buf.append(data.clone());
                self.file_size = buf.right_offset;
                self.file.seek(SeekFrom::End(0))?;
                self.file.write_all(data)?;
                return Ok(addr);
            }
        }

        let start = self.file.seek(SeekFrom::End(0))?;
        let new_file_size = start + data.len() as u64;
        self.file.write_all(data)?;
        self.file_size = new_file_size;
        Ok(start)
    }

    /// Updates the index at the given address with the new data.
    ///
    /// # Errors
    /// - This will fail if the data could spill into the key-value entry section or in the header section e.g.
    /// if the address is less than [HEADER_SIZE_IN_BYTES]
    /// or (address + data length) is greater than or equal [BufferPool.key_values_start_point]
    pub(crate) fn update_index(&mut self, address: u64, data: &[u8]) -> io::Result<()> {
        validate_bounds!(
            (address, address + data.len() as u64),
            (HEADER_SIZE_IN_BYTES, self.key_values_start_point),
            "The data is outside the index bounds"
        )?;

        for (_, buf) in self.index_buffers.iter_mut() {
            if buf.contains(address) {
                buf.replace(address, data.to_vec())?;
            }
        }

        self.file.seek(SeekFrom::Start(address))?;
        self.file.write_all(data)?;

        Ok(())
    }

    /// Clears all data on disk and memory making it like a new store
    pub(crate) fn clear_file(&mut self) -> io::Result<()> {
        let header = DbFileHeader::new(self.max_keys, self.redundant_blocks, None);
        self.file_size = initialize_db_file(&mut self.file, &header)?;
        self.index_buffers.clear();
        self.kv_buffers.clear();
        Ok(())
    }

    /// This removes any deleted or expired entries from the file. It must first lock the buffer and the file.
    /// In order to be more efficient, it creates a new file, copying only that data which is not deleted or expired
    pub(crate) fn compact_file(&mut self) -> io::Result<()> {
        let folder = self.file_path.parent().unwrap_or_else(|| Path::new("/"));
        let new_file_path = folder.join("tmp__compact.scdb");
        let mut new_file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(&new_file_path)?;

        let header: DbFileHeader = DbFileHeader::from_file(&mut self.file)?;

        // Add headers to new file
        new_file.seek(SeekFrom::Start(0))?;
        new_file.write_all(&header.as_bytes())?;

        let file = Mutex::new(&self.file);

        let mut index = Index::new(&file, &header);

        let idx_entry_size = INDEX_ENTRY_SIZE_IN_BYTES as usize;
        let zero = vec![0u8; idx_entry_size];
        let mut idx_offset = HEADER_SIZE_IN_BYTES;
        let mut new_file_offset = header.key_values_start_point;

        for index_block in &mut index {
            let index_block = index_block?;
            // write index block into new file
            new_file.seek(SeekFrom::Start(idx_offset))?;
            new_file.write_all(&index_block)?;

            let len = index_block.len();
            let mut idx_block_cursor: usize = 0;
            while idx_block_cursor < len {
                let lower = idx_block_cursor;
                let upper = lower + idx_entry_size;
                let idx_bytes = index_block[lower..upper].to_vec();

                if idx_bytes != zero {
                    let kv_byte_array = get_kv_bytes(&file, &idx_bytes)?;
                    let kv = KeyValueEntry::from_data_array(&kv_byte_array, 0)?;
                    if !kv.is_expired() && !kv.is_deleted {
                        let kv_size = kv_byte_array.len() as u64;
                        // insert key value
                        new_file.seek(SeekFrom::Start(new_file_offset))?;
                        new_file.write_all(&kv_byte_array)?;

                        // update index
                        new_file.seek(SeekFrom::Start(idx_offset))?;
                        new_file.write_all(&new_file_offset.to_be_bytes())?;
                        new_file_offset += kv_size;
                    } else {
                        // if expired or deleted, update index to zero
                        new_file.seek(SeekFrom::Start(idx_offset))?;
                        new_file.write_all(&zero)?;
                    }
                }

                idx_block_cursor = upper;
                idx_offset += INDEX_ENTRY_SIZE_IN_BYTES;
            }
        }

        self.kv_buffers.clear();
        self.index_buffers.clear();
        self.file = new_file;
        self.file_size = new_file_offset;

        fs::remove_file(&self.file_path)?;
        fs::rename(&new_file_path, &self.file_path)?;

        Ok(())
    }

    /// Returns the Some(Value) at the given address if the key there corresponds to the given key
    /// Otherwise, it returns None
    /// This is to handle hash collisions.
    pub(crate) fn get_value(&mut self, kv_address: u64, key: &[u8]) -> io::Result<Option<Value>> {
        if kv_address == 0 {
            return Ok(None);
        }

        // loop in reverse, starting at the back
        // since the latest kv_buffers are the ones updated when new changes occur
        for buf in self.kv_buffers.iter_mut().rev() {
            if buf.contains(kv_address) {
                return buf.get_value(kv_address, key);
            }
        }

        if self.kv_buffers.len() >= self.kv_capacity {
            self.kv_buffers.pop_front();
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        self.file.seek(SeekFrom::Start(kv_address))?;
        let bytes_read = self.file.read(&mut buf)?;

        // update kv_buffers only upto actual data read (cater for partially filled buffer)
        self.kv_buffers.push_back(Buffer::new(
            kv_address,
            &buf[..bytes_read],
            self.buffer_size,
        ));

        let entry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = if entry.key == key && !entry.is_expired() {
            Some(Value::from(&entry))
        } else {
            None
        };

        Ok(value)
    }

    /// Attempts to delete the key-value entry for the given kv_address as long as the key it holds
    /// is the same as the key provided
    pub(crate) fn try_delete_kv_entry(
        &mut self,
        kv_address: u64,
        key: &[u8],
    ) -> io::Result<Option<()>> {
        let key_size = key.len();
        let addr_for_is_deleted = kv_address + OFFSET_FOR_KEY_IN_KV_ARRAY as u64 + key_size as u64;
        // loop in reverse, starting at the back
        // since the latest kv_buffers are the ones updated when new changes occur
        for buf in self.kv_buffers.iter_mut().rev() {
            if buf.contains(kv_address) && buf.try_delete_kv_entry(kv_address, key)?.is_some() {
                self.file.seek(SeekFrom::Start(addr_for_is_deleted))?;
                self.file.write_all(&[TRUE_AS_BYTE])?;
                return Ok(Some(()));
            }
        }

        let key_in_data =
            extract_key_as_byte_array_from_file(&mut self.file, kv_address, key_size)?;
        if key_in_data == key {
            self.file.seek(SeekFrom::Start(addr_for_is_deleted))?;
            self.file.write_all(&[TRUE_AS_BYTE])?;
            Ok(Some(()))
        } else {
            Ok(None)
        }
    }

    /// Checks to see if the given kv address is for the given key.
    /// Note that this returns true for expired keys as long as compaction has not yet been done.
    /// This avoids duplicate entries for the same key being tracked in separate index entries
    ///
    /// It also returns false if the address goes beyond the size of the file
    pub(crate) fn addr_belongs_to_key(
        &mut self,
        kv_address: &[u8],
        key: &[u8],
    ) -> io::Result<bool> {
        let kv_address = u64::from_be_bytes(slice_to_array(kv_address)?);
        if kv_address >= self.file_size {
            return Ok(false);
        }

        // loop in reverse, starting at the back
        // since the latest kv_buffers are the ones updated when new changes occur
        for buf in self.kv_buffers.iter_mut().rev() {
            if buf.contains(kv_address) {
                return buf.addr_belongs_to_key(kv_address, key);
            }
        }

        if self.kv_buffers.len() >= self.kv_capacity {
            self.kv_buffers.pop_front();
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        self.file.seek(SeekFrom::Start(kv_address))?;
        let bytes_read = self.file.read(&mut buf)?;

        // update kv_buffers only upto actual data read (cater for partially filled buffer)
        self.kv_buffers.push_back(Buffer::new(
            kv_address,
            &buf[..bytes_read],
            self.buffer_size,
        ));

        let key_in_file = &buf[OFFSET_FOR_KEY_IN_KV_ARRAY..OFFSET_FOR_KEY_IN_KV_ARRAY + key.len()];
        let value = key_in_file == key;
        Ok(value)
    }

    /// Reads the index at the given address and returns it
    ///
    /// # Errors
    ///
    /// If the address is less than [HEADER_SIZE_IN_BYTES] or [BufferPool.key_values_start_point],
    /// an InvalidData error is returned
    pub(crate) fn read_index(&mut self, address: u64) -> io::Result<Vec<u8>> {
        validate_bounds!(
            (address, address + INDEX_ENTRY_SIZE_IN_BYTES),
            (HEADER_SIZE_IN_BYTES, self.key_values_start_point)
        )?;

        let size = INDEX_ENTRY_SIZE_IN_BYTES as usize;
        let mut last_buf: Option<u64> = None;
        // starts from buffer with lowest left_offset, which I expect to have more keys
        for (i, buf) in self.index_buffers.iter() {
            if buf.contains(address) {
                return buf.read_at(address, size);
            }
            last_buf.replace(*i);
        }

        if self.index_buffers.len() >= self.index_capacity {
            if let Some(k) = last_buf {
                self.index_buffers.remove(&k);
            }
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        self.file.seek(SeekFrom::Start(address))?;
        let bytes_read = self.file.read(&mut buf)?;

        // update index_buffers only upto actual data read (cater for partially filled buffer)
        self.index_buffers.insert(
            address,
            Buffer::new(address, &buf[..bytes_read], self.buffer_size),
        );

        let data_array = buf[0..size].to_vec();
        Ok(data_array)
    }

    /// Gets all the key-value pairs that correspond to the given list of key-value addresses
    pub(crate) fn get_many_key_values(
        &self,
        kv_addresses: &[u64],
    ) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        todo!()
    }
}

impl PartialEq for BufferPool {
    fn eq(&self, other: &Self) -> bool {
        self.kv_capacity == other.kv_capacity
            && self.index_capacity == other.index_capacity
            && self.key_values_start_point == other.key_values_start_point
            && self.buffer_size == other.buffer_size
            && self.max_keys == other.max_keys
            && self.redundant_blocks == other.redundant_blocks
            && self.file_path == other.file_path
            && self.file_size == other.file_size
            && self.kv_buffers == other.kv_buffers
            && self.index_buffers == other.index_buffers
    }
}

impl Display for BufferPool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BufferPool {{kv_capacity: {}, index_capacity: {}, buffer_size: {}, key_values_start_point: {}, max_keys: {:?}, redundant_blocks: {:?}, kv_buffers: {:?}, index_buffers: {:?}, file: {:?}, file_path: {:?}, file_size: {}}}",
            self.kv_capacity,
            self.index_capacity,
            self.buffer_size,
            self.key_values_start_point,
            self.max_keys,
            self.redundant_blocks,
            self.kv_buffers,
            self.index_buffers,
            self.file,
            self.file_path,
            self.file_size,
        )
    }
}

/// Extracts the byte array for the key from a given file
fn extract_key_as_byte_array_from_file(
    file: &mut File,
    kv_address: u64,
    key_size: usize,
) -> io::Result<Vec<u8>> {
    let offset = kv_address + OFFSET_FOR_KEY_IN_KV_ARRAY as u64;
    let mut buf: Vec<u8> = vec![0; key_size];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Computes the capacity (i.e. number of buffers) of the buffers to be set aside for index buffers
/// It can't be less than 1 and it can't be more than the number of index blocks available
#[inline]
fn get_index_capacity(num_of_index_blocks: usize, capacity: usize) -> usize {
    let index_capacity = (2.0 * capacity as f64 / 3.0).floor() as usize;
    max(1, min(num_of_index_blocks, index_capacity))
}

/// Reads a byte array for a key-value entry at the given address in the file
fn get_kv_bytes(file: &Mutex<&File>, address: &[u8]) -> io::Result<Vec<u8>> {
    let mut file = acquire_lock!(file)?;
    let address = u64::from_be_bytes(slice_to_array(address)?);

    // get size of the whole key value entry
    let mut size_bytes: [u8; 4] = [0; 4];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut size_bytes)?;
    let size = u32::from_be_bytes(size_bytes);

    // get the key value entry itself, basing on the size it has
    let mut data = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut data)?;

    Ok(data)
}

/// Initializes the database file, giving it the header and the index place holders
/// and truncating it. It returns the new file size
fn initialize_db_file(file: &mut File, header: &DbFileHeader) -> io::Result<u64> {
    let header_bytes = header.as_bytes();
    let header_length = header_bytes.len() as u64;
    debug_assert_eq!(header_length, 100);
    let final_size = header_length + (header.number_of_index_blocks * header.net_block_size);

    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header_bytes)?;
    // shrink file if it is still long
    file.set_len(header_length)?;
    // expand file to expected value, filling the extras with 0s
    file.set_len(final_size)?;
    Ok(final_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::entries::values::key_value::KEY_VALUE_MIN_SIZE_IN_BYTES;
    use crate::internal::get_current_timestamp;
    use serial_test::serial;

    #[test]
    #[serial]
    fn new_with_non_existing_file() {
        type Config<'a> = (
            Option<usize>,
            &'a Path,
            Option<u64>,
            Option<u16>,
            Option<usize>,
        );

        let file_name = "testdb.scdb";

        struct Expected {
            buffer_size: usize,
            max_keys: Option<u64>,
            redundant_blocks: Option<u16>,
            file_path: PathBuf,
            file_size: u64,
        }

        let test_data: Vec<(Config<'_>, Expected)> = vec![
            (
                (None, &Path::new(file_name), None, None, None),
                Expected {
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: None,
                    file_path: Path::new(file_name).into(),
                    file_size: DbFileHeader::new(None, None, None).key_values_start_point,
                },
            ),
            (
                (Some(60), &Path::new(file_name), None, None, None),
                Expected {
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: None,
                    file_path: Path::new(file_name).into(),
                    file_size: DbFileHeader::new(None, None, None).key_values_start_point,
                },
            ),
            (
                (None, &Path::new(file_name), Some(360), None, None),
                Expected {
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: Some(360),
                    redundant_blocks: None,
                    file_path: Path::new(file_name).into(),
                    file_size: DbFileHeader::new(Some(360), None, None).key_values_start_point,
                },
            ),
            (
                (None, &Path::new(file_name), None, Some(4), None),
                Expected {
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: Some(4),
                    file_path: Path::new(file_name).into(),
                    file_size: DbFileHeader::new(None, Some(4), None).key_values_start_point,
                },
            ),
            (
                (None, &Path::new(file_name), None, None, Some(2048)),
                Expected {
                    buffer_size: 2048,
                    max_keys: None,
                    redundant_blocks: None,
                    file_path: Path::new(file_name).into(),
                    file_size: DbFileHeader::new(None, None, Some(2048)).key_values_start_point,
                },
            ),
        ];

        // delete the file so that BufferPool::new() can reinitialize it.
        fs::remove_file(&file_name).ok();

        for ((capacity, file_path, max_keys, redundant_blocks, buffer_size), expected) in test_data
        {
            let got = BufferPool::new(capacity, file_path, max_keys, redundant_blocks, buffer_size)
                .expect("new buffer pool");

            assert_eq!(&got.buffer_size, &expected.buffer_size);
            assert_eq!(&got.max_keys, &expected.max_keys);
            assert_eq!(&got.redundant_blocks, &expected.redundant_blocks);
            assert_eq!(&got.file_path, &expected.file_path);
            assert_eq!(&got.file_size, &expected.file_size);

            // delete the file so that BufferPool::new() can reinitialize it for the next iteration
            fs::remove_file(&got.file_path).expect(&format!("delete file {:?}", &got.file_path));
        }
    }

    #[test]
    #[serial]
    fn new_with_existing_file() {
        type Config<'a> = (
            Option<usize>,
            &'a Path,
            Option<u64>,
            Option<u16>,
            Option<usize>,
        );
        let file_name = "testdb.scdb";
        let test_data: Vec<Config<'_>> = vec![
            (None, &Path::new(file_name), None, None, None),
            (Some(60), &Path::new(file_name), None, None, None),
            (None, &Path::new(file_name), Some(360), None, None),
            (None, &Path::new(file_name), None, Some(4), None),
            (None, &Path::new(file_name), None, None, Some(2048)),
        ];

        for (capacity, file_path, max_keys, redundant_blocks, buffer_size) in test_data {
            let first =
                BufferPool::new(capacity, file_path, max_keys, redundant_blocks, buffer_size)
                    .expect("new buffer pool");
            let second =
                BufferPool::new(capacity, file_path, max_keys, redundant_blocks, buffer_size)
                    .expect("new buffer pool");
            assert_eq!(&first, &second);
            // delete the file so that BufferPool::new() can reinitialize it for the next iteration
            fs::remove_file(&first.file_path)
                .expect(&format!("delete file {:?}", &first.file_path));
        }
    }

    #[test]
    #[serial]
    fn append_to_file() {
        let file_name = "testdb.scdb";
        let mut data = vec![72u8, 97, 108, 108, 101, 108, 117, 106, 97, 104];
        let data_length = data.len();
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");
        let initial_file_size = get_pool_file_size(&mut pool);

        pool.append(&mut data).expect("append data");

        let final_file_size = get_pool_file_size(&mut pool);
        let (data_in_file, bytes_read) = read_from_file(file_name, initial_file_size, data_length);
        let actual_file_size = get_actual_file_size(file_name);

        assert_eq!(final_file_size, initial_file_size + data_length as u64);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, data_length);
        assert_eq!(data_in_file, data);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn append_to_pre_existing_buffer() {
        let file_name = "testdb.scdb";
        let initial_data = &[76u8, 67, 56];
        let initial_data_length = initial_data.len() as u64;
        let mut data = vec![72u8, 97, 108, 108, 101, 108, 117, 106, 97, 104];
        let data_length = data.len();

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let initial_offset = get_actual_file_size(file_name);
        write_to_file(file_name, initial_offset, initial_data);
        increment_pool_file_size(&mut pool, initial_data_length);
        let (header_array, _) = read_from_file(file_name, 0, 100);
        let initial_file_size = get_pool_file_size(&mut pool);
        append_kv_buffers(
            &mut pool,
            &[(initial_offset, &initial_data[..]), (0, &header_array[..])][..],
        );

        pool.append(&mut data).expect("appends data to buffer pool");

        let (data_in_file, bytes_read) =
            read_from_file(file_name, initial_offset + initial_data_length, data_length);
        let actual_file_size = get_actual_file_size(file_name);
        let final_file_size = get_pool_file_size(&mut pool);

        let first_buf = pool.kv_buffers.pop_front().expect("buffer popped front");

        // assert things in file
        assert_eq!(final_file_size, initial_file_size + data_length as u64);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, data_length);
        assert_eq!(data_in_file, data);

        // assert things in buffer
        assert_eq!(first_buf.right_offset, final_file_size);
        assert_eq!(first_buf.data, [initial_data.to_vec(), data].concat());

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn update_index_in_file() {
        let file_name = "testdb.scdb";
        let old_index: u64 = 890;
        let new_index: u64 = 6987;
        let data = old_index.to_be_bytes();
        let data_length = data.len();
        let new_data = new_index.to_be_bytes();
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");
        let offset = HEADER_SIZE_IN_BYTES + 5;
        let initial_file_size = get_pool_file_size(&mut pool);
        write_to_file(file_name, offset, &data);

        pool.update_index(offset, &mut new_data.to_vec())
            .expect("replace data");

        let final_file_size = get_pool_file_size(&mut pool);
        let (data_in_file, bytes_read) = read_from_file(file_name, offset, data_length);
        let actual_file_size = get_actual_file_size(file_name);

        assert_eq!(final_file_size, initial_file_size);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, data_length);
        assert_eq!(data_in_file, new_data);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn update_index_in_pre_existing_buffer() {
        let file_name = "testdb.scdb";

        let old_index: u64 = 890;
        let new_index: u64 = 6783;
        let initial_data = old_index.to_be_bytes();
        let mut new_data = new_index.to_be_bytes();
        let new_data_length = new_data.len();

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let initial_offset = HEADER_SIZE_IN_BYTES + 4;
        let initial_file_size = get_pool_file_size(&mut pool);

        write_to_file(file_name, initial_offset, &initial_data);
        append_index_buffers(&mut pool, &[(initial_offset, &initial_data[..])][..]);

        pool.update_index(initial_offset, &mut new_data)
            .expect("replaces data in buffer");

        let (data_in_file, bytes_read) = read_from_file(file_name, initial_offset, new_data_length);
        let actual_file_size = get_actual_file_size(file_name);
        let final_file_size = get_pool_file_size(&mut pool);

        let buf = pool.index_buffers.get(&initial_offset).expect("get buffer");

        // assert things in file
        assert_eq!(final_file_size, initial_file_size);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, new_data_length);
        assert_eq!(data_in_file, new_data);

        // assert things in buffer
        assert_eq!(buf.right_offset, initial_offset + new_data_length as u64);
        assert_eq!(buf.data, new_data);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn update_index_out_of_bounds() {
        let file_name = "testdb.scdb";

        let old_index: u64 = 890;
        let new_index: u64 = 6783;
        let initial_data = old_index.to_be_bytes();
        let mut new_data = new_index.to_be_bytes();

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        append_index_buffers(
            &mut pool,
            &[(HEADER_SIZE_IN_BYTES + 2, &initial_data[..])][..],
        );

        let addresses = &[
            pool.key_values_start_point + 3,
            pool.key_values_start_point + 50,
            HEADER_SIZE_IN_BYTES - 6,
        ];

        for address in addresses {
            let response = pool.update_index(*address, &mut new_data);
            assert!(response.is_err());
        }

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn clear_file_works() {
        let file_name = "testdb.scdb";
        let initial_data = &[76u8, 67, 56];
        let initial_data_length = initial_data.len() as u64;

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");
        let expected = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let initial_offset = get_actual_file_size(file_name);
        write_to_file(file_name, initial_offset, initial_data);
        increment_pool_file_size(&mut pool, initial_data_length);
        let (header_array, _) = read_from_file(file_name, 0, 100);
        append_kv_buffers(
            &mut pool,
            &[(initial_offset, &initial_data[..]), (0, &header_array[..])][..],
        );

        pool.clear_file().expect("file cleared");
        assert_eq!(&pool, &expected);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn compact_file_works() {
        let file_name = "testdb.scdb";
        // pre-clean up for right results
        fs::remove_file(&file_name).ok();

        let never_expires = KeyValueEntry::new(&b"never_expires"[..], &b"bar"[..], 0);
        let deleted = KeyValueEntry::new(&b"deleted"[..], &b"bok"[..], 0);
        // 1666023836u64 is some past timestamp in October 2022
        let expired = KeyValueEntry::new(&b"expires"[..], &b"bar"[..], 1666023836u64);
        let not_expired = KeyValueEntry::new(
            &b"not_expired"[..],
            &b"bar"[..],
            get_current_timestamp() * 2,
        );
        // Limit the max_keys to 10 otherwise the memory will be consumed when we try to get all data in file
        let mut pool = BufferPool::new(None, &Path::new(file_name), Some(10), Some(1), None)
            .expect("new buffer pool");

        append_kv_buffers(&mut pool, &[(0, &[76u8, 79][..])][..]);

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        // insert key value pairs in pool
        insert_key_value_entry(&mut pool, &header, &never_expires);
        insert_key_value_entry(&mut pool, &header, &deleted);
        insert_key_value_entry(&mut pool, &header, &expired);
        insert_key_value_entry(&mut pool, &header, &not_expired);

        // delete the key-value to be deleted
        delete_key_value(&mut pool, &header, &deleted);

        let initial_file_size = get_actual_file_size(file_name);

        pool.compact_file().expect("compact file");

        let final_file_size = get_actual_file_size(file_name);
        let (data_in_file, _) = read_from_file(file_name, 0, final_file_size as usize);
        let pool_file_size = get_pool_file_size(&mut pool);

        let buffer_len = pool.kv_buffers.len();

        let expected_file_size_reduction = deleted.size as u64 + expired.size as u64;
        let expired_kv_address = get_kv_address(&mut pool, &header, &expired);
        let deleted_kv_address = get_kv_address(&mut pool, &header, &deleted);

        assert_eq!(buffer_len, 0);
        assert_eq!(pool_file_size, final_file_size);
        assert_eq!(
            initial_file_size - final_file_size,
            expected_file_size_reduction
        );
        assert_eq!(expired_kv_address, 0);
        assert_eq!(deleted_kv_address, 0);

        assert!(key_value_exists(&data_in_file, &header, &never_expires));
        assert!(key_value_exists(&data_in_file, &header, &not_expired));
        assert!(!key_value_exists(&data_in_file, &header, &deleted));
        assert!(!key_value_exists(&data_in_file, &header, &expired));

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn get_value_works() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"kv"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);

        let kv_address = get_kv_address(&mut pool, &header, &kv);
        let got = pool
            .get_value(kv_address, kv.key)
            .expect("get value")
            .unwrap();
        let expected = Value::from(&kv);

        assert_eq!(got, expected);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn get_value_from_buffer() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"kv"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);

        let kv_address = get_kv_address(&mut pool, &header, &kv);

        let _ = pool
            .get_value(kv_address, kv.key)
            .expect("get value first time")
            .unwrap();

        // delete underlying file first
        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));

        // the second get must be getting value from memory
        let got = pool
            .get_value(kv_address, kv.key)
            .expect("get value second time")
            .unwrap();

        let expected = Value::from(&kv);

        assert_eq!(got, expected);
    }

    #[test]
    #[serial]
    fn get_value_expired() {
        let file_name = "testdb.scdb";
        // 1666023836u64 is some past timestamp in October 2022 so this is expired
        let kv = KeyValueEntry::new(&b"expires"[..], &b"bar"[..], 1666023836u64);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);

        let kv_address = get_kv_address(&mut pool, &header, &kv);
        let got = pool.get_value(kv_address, kv.key).expect("get value");

        assert!(got.is_none());

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn get_value_deleted() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"deleted"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);
        delete_key_value(&mut pool, &header, &kv);

        let kv_address = get_kv_address(&mut pool, &header, &kv);
        assert_eq!(kv_address, 0u64);

        let got = pool.get_value(kv_address, kv.key).expect("get value");
        assert!(got.is_none());

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn addr_belongs_to_key_works() {
        let file_name = "testdb.scdb";
        let kv1 = KeyValueEntry::new(&b"never"[..], &b"bar"[..], 0);
        let kv2 = KeyValueEntry::new(&b"foo"[..], &b"baracuda"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv1);
        insert_key_value_entry(&mut pool, &header, &kv2);

        let kv1_index_address = get_kv_address_as_bytes(&mut pool, &header, &kv1);
        let kv2_index_address = get_kv_address_as_bytes(&mut pool, &header, &kv2);
        assert!(pool
            .addr_belongs_to_key(&kv1_index_address, kv1.key)
            .expect("addr_belongs_to_key kv1"));
        assert!(pool
            .addr_belongs_to_key(&kv2_index_address, kv2.key)
            .expect("addr_belongs_to_key kv2"));
        assert!(!pool
            .addr_belongs_to_key(&kv1_index_address, kv2.key)
            .expect("addr_belongs_to_key kv1"));
        assert!(!pool
            .addr_belongs_to_key(&kv2_index_address, kv1.key)
            .expect("addr_belongs_to_key kv2"));

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn addr_belongs_to_key_expired_returns_true() {
        let file_name = "testdb.scdb";
        // 1666023836u64 is some past timestamp in October 2022 so this is expired
        let kv = KeyValueEntry::new(&b"expires"[..], &b"bar"[..], 1666023836u64);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);
        let kv_index_address = get_kv_address_as_bytes(&mut pool, &header, &kv);

        assert!(pool
            .addr_belongs_to_key(&kv_index_address, kv.key)
            .expect("addr_belongs_to_key kv"));

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn addr_belongs_to_key_works_out_of_bounds() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);
        let file_size = get_actual_file_size(file_name);
        let file_size = file_size.to_be_bytes();

        assert!(!pool
            .addr_belongs_to_key(&file_size, kv.key)
            .expect("addr_belongs_to_key kv"));

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn try_delete_kv_entry_works() {
        let file_name = "testdb.scdb";
        let kv1 = KeyValueEntry::new(&b"never"[..], &b"bar"[..], 0);
        let kv2 = KeyValueEntry::new(&b"foo"[..], &b"baracuda"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv1);
        insert_key_value_entry(&mut pool, &header, &kv2);

        let kv1_index_address = get_kv_address(&mut pool, &header, &kv1);

        let resp = pool
            .try_delete_kv_entry(kv1_index_address, &kv2.key)
            .expect("try delete kv1 with kv2 key");
        assert!(resp.is_none());
        assert_eq!(
            pool.get_value(kv1_index_address, &kv1.key).unwrap(),
            Some(Value {
                data: vec![98u8, 97, 114],
                is_stale: false,
            })
        );

        let resp = pool
            .try_delete_kv_entry(kv1_index_address, &kv1.key)
            .expect("try delete kv1 with kv1 key");
        assert!(resp.is_some());
        assert_eq!(
            pool.get_value(kv1_index_address, &kv1.key).unwrap(),
            Some(Value {
                data: vec![98u8, 97, 114],
                is_stale: true,
            })
        );

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn read_index_works() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"kv"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);

        let index_address = header.get_index_offset(kv.key);
        let kv_address = get_kv_address(&mut pool, &header, &kv);

        assert_eq!(
            pool.read_index(index_address).expect("read_at for index")[..],
            kv_address.to_be_bytes()
        );

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    #[serial]
    fn read_at_works_out_of_bounds() {
        let file_name = "testdb.scdb";
        let kv = KeyValueEntry::new(&b"kv"[..], &b"bar"[..], 0);
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let header = DbFileHeader::from_file(&mut pool.file).expect("get header");

        insert_key_value_entry(&mut pool, &header, &kv);

        let kv_address = get_kv_address(&mut pool, &header, &kv);
        let value_address = kv_address + KEY_VALUE_MIN_SIZE_IN_BYTES as u64 + kv.key_size as u64;
        let file_size = get_actual_file_size(file_name);

        let test_data = vec![kv_address, value_address, file_size];

        for addr in test_data {
            assert!(pool.read_index(addr).is_err());
        }

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    /// Returns the actual file size of the file at the given path
    fn get_actual_file_size(file_path: &str) -> u64 {
        let mut file = OpenOptions::new()
            .read(true)
            .open(file_path)
            .expect(&format!("open file {}", file_path));
        file.seek(SeekFrom::End(0)).expect("get file size")
    }

    /// Extracts the pool's file_size attribute
    fn get_pool_file_size(pool: &mut BufferPool) -> u64 {
        let initial_file_size = pool.file_size;
        initial_file_size
    }

    /// Manually increments the pool's file_size attribute
    fn increment_pool_file_size(pool: &mut BufferPool, incr: u64) {
        pool.file_size += incr;
    }

    /// Reads from the file at the given file path at the given offset returning the number of bytes read
    /// and the data itself
    fn read_from_file(file_name: &str, addr: u64, buf_size: usize) -> (Vec<u8>, usize) {
        let mut file = OpenOptions::new()
            .read(true)
            .open(file_name)
            .expect(&format!("open the file: {}", file_name));
        file.seek(SeekFrom::Start(addr))
            .expect(&format!("seek to addr {}", addr));

        let mut data_in_file: Vec<u8> = vec![0; buf_size];
        let bytes_read = file.read(&mut data_in_file).expect("read file");

        (data_in_file, bytes_read)
    }

    /// Writes the given data to the file at the given address
    fn write_to_file(file_path: &str, addr: u64, data: &[u8]) {
        let mut file = OpenOptions::new()
            .write(true)
            .open(file_path)
            .expect(&format!("open the file: {}", file_path));

        file.seek(SeekFrom::Start(addr))
            .expect(&format!("seek to {}", addr));

        file.write_all(data).expect("write all data to file");
    }

    /// Creates and appends key-value buffers to the pool from the offset-data pairs
    fn append_kv_buffers(pool: &mut BufferPool, pairs: &[(u64, &[u8])]) {
        for (offset, data) in pairs {
            pool.kv_buffers
                .push_back(Buffer::new(*offset, data, pool.buffer_size));
        }
    }

    /// Creates and appends index buffers to the pool from the offset-data pairs
    fn append_index_buffers(pool: &mut BufferPool, pairs: &[(u64, &[u8])]) {
        for (offset, data) in pairs {
            pool.index_buffers
                .insert(*offset, Buffer::new(*offset, data, pool.buffer_size));
        }
    }

    /// Deletes a given key value in the given pool
    fn delete_key_value(pool: &mut BufferPool, header: &DbFileHeader, kv: &KeyValueEntry<'_>) {
        let addr = header.get_index_offset(kv.key);
        pool.update_index(addr, &0u64.to_be_bytes())
            .expect("replace deleted key with empty");
    }

    /// Inserts a key value entry into the pool, updating the index also
    fn insert_key_value_entry(
        pool: &mut BufferPool,
        header: &DbFileHeader,
        kv: &KeyValueEntry<'_>,
    ) {
        let idx_addr = header.get_index_offset(kv.key);
        let kv_addr = pool
            .append(&mut kv.as_bytes())
            .expect(&format!("inserts key value {:?}", &kv));

        pool.update_index(idx_addr, &kv_addr.to_be_bytes())
            .expect(&format!("updates index of {:?}", &kv));
    }

    /// Checks whether a given key value entry exists in the data array got from the file
    fn key_value_exists(data: &Vec<u8>, header: &DbFileHeader, kv: &KeyValueEntry<'_>) -> bool {
        let idx_item_size = INDEX_ENTRY_SIZE_IN_BYTES as usize;
        let idx_addr = header.get_index_offset(kv.key) as usize;
        let kv_addr = data[idx_addr..idx_addr + idx_item_size].to_vec();
        if kv_addr != vec![0u8; idx_item_size] {
            let kv_addr = u64::from_be_bytes(slice_to_array(&kv_addr[..]).expect("slice to array"));
            match KeyValueEntry::from_data_array(data, kv_addr as usize) {
                Ok(_) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Returns the address byte array for the given key value entry within the buffer pool
    fn get_kv_address_as_bytes(
        pool: &mut BufferPool,
        header: &DbFileHeader,
        kv: &KeyValueEntry<'_>,
    ) -> Vec<u8> {
        let mut kv_address = vec![0u8; INDEX_ENTRY_SIZE_IN_BYTES as usize];
        let index_address = header.get_index_offset(kv.key);

        pool.file
            .seek(SeekFrom::Start(index_address))
            .expect("seek to index");
        pool.file
            .read(&mut kv_address)
            .expect("reads value at index address");

        kv_address
    }

    /// Returns the address for the given key value entry within the buffer pool
    fn get_kv_address(pool: &mut BufferPool, header: &DbFileHeader, kv: &KeyValueEntry<'_>) -> u64 {
        let kv_address = get_kv_address_as_bytes(pool, header, kv);
        u64::from_be_bytes(slice_to_array(&kv_address[..]).expect("slice to array"))
    }
}
