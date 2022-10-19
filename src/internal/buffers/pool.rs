use crate::internal::buffers::buffer::{Buffer, Value};
use crate::internal::utils::get_vm_page_size;
use crate::internal::{acquire_lock, slice_to_array, DbFileHeader, KeyValueEntry};
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::DerefMut;
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
    capacity: usize,
    buffer_size: usize,
    max_keys: Option<u64>,
    redundant_blocks: Option<u16>,
    // These are used only for reads
    buffers: Mutex<VecDeque<Buffer>>,
    pub(crate) file: Mutex<File>,
    file_path: PathBuf,
    pub(crate) file_size: Mutex<u64>,
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
        let buffer_size = match buffer_size {
            None => get_vm_page_size() as usize,
            Some(v) => v,
        };

        let capacity = match capacity {
            None => DEFAULT_POOL_CAPACITY,
            Some(v) => v,
        };

        let should_create_new = !file_path.exists();
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(should_create_new)
            .open(file_path)?;

        if should_create_new {
            let header = DbFileHeader::new(max_keys, redundant_blocks);
            initialize_db_file(&mut file, &header, None)?;
        };

        let file_size = file.seek(SeekFrom::End(0))?;

        let v = Self {
            capacity,
            buffer_size,
            max_keys,
            redundant_blocks,
            buffers: Mutex::new(VecDeque::with_capacity(capacity)),
            file: Mutex::new(file),
            file_size: Mutex::new(file_size),
            file_path: file_path.into(),
        };

        Ok(v)
    }

    /// Appends a given data array to the file attached to this buffer pool
    /// It returns the address where the data was appended
    pub(crate) fn append(&mut self, data: &mut Vec<u8>) -> io::Result<u64> {
        let mut file = acquire_lock!(self.file)?;
        let mut buffers = acquire_lock!(self.buffers)?;
        let mut file_size = acquire_lock!(self.file_size)?;

        // loop in reverse, starting at the back
        // since the latest buffers are the ones updated when new changes occur
        for buf in buffers.iter_mut().rev() {
            if buf.can_append(*file_size) {
                let addr = buf.append(data.clone());
                *file_size = buf.right_offset;
                file.seek(SeekFrom::End(0))?;
                file.write_all(data)?;
                return Ok(addr);
            }
        }

        let start = file.seek(SeekFrom::End(0))?;
        let new_file_size = start + data.len() as u64;
        file.write_all(data)?;
        *file_size = new_file_size;
        Ok(start)
    }

    /// Inserts a given data array at the given address. Do note that this overwrites
    /// the existing data at that address. If you are looking to update to a value that
    /// could have a different length from the previous one, append it to the bottom
    /// then overwrite the previous offset in the index with the offset of the new entry
    pub(crate) fn replace(&mut self, address: u64, data: &[u8]) -> io::Result<()> {
        let file_size = acquire_lock!(self.file_size)?;
        self.validate_bounds(address, address + data.len() as u64, *file_size)?;

        let mut file = acquire_lock!(self.file)?;
        let mut buffers = acquire_lock!(self.buffers)?;

        // loop in reverse, starting at the back
        // since the latest buffers are the ones updated when new changes occur
        for buf in buffers.iter_mut().rev() {
            if buf.contains(address) {
                buf.replace(address, data.to_vec())?;
                file.seek(SeekFrom::Start(address))?;
                file.write_all(data)?;
                return Ok(());
            }
        }

        file.seek(SeekFrom::Start(address))?;
        file.write_all(data)?;

        Ok(())
    }

    /// Clears all data on disk and memory making it like a new store
    pub(crate) fn clear_file(&mut self) -> io::Result<()> {
        let header = DbFileHeader::new(self.max_keys, self.redundant_blocks);
        let mut file = acquire_lock!(self.file)?;
        let mut buffers = acquire_lock!(self.buffers)?;
        let mut file_size = acquire_lock!(self.file_size)?;
        *file_size = reinitialize_db_file(file.deref_mut(), &header)?;
        buffers.clear();
        Ok(())
    }

    /// This removes any deleted or expired entries from the file. It must first lock the buffer and the file.
    /// In order to be more efficient, it creates a new file, copying only that data which is not deleted or expired
    pub(crate) fn compact_file(&mut self) -> io::Result<()> {
        let folder = match self.file_path.parent() {
            None => Path::new("/"),
            Some(v) => v,
        };
        let new_file_path = folder.join("tmp__compact.scdb");
        let mut new_file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(&new_file_path)?;

        let mut file = acquire_lock!(self.file)?;
        let mut buffers = acquire_lock!(self.buffers)?;
        let mut file_size = acquire_lock!(self.file_size)?;

        let file_ref = file.deref_mut();
        let header: DbFileHeader = DbFileHeader::from_file(file_ref)?;
        let index_bytes_array = get_index_as_byte_array(file_ref, &header)?;
        let reversed_index_map: HashMap<u64, u64> = get_index_as_reversed_map(&index_bytes_array)?;
        initialize_db_file(&mut new_file, &header, Some(index_bytes_array))?;

        let mut curr_offset = header.key_values_start_point;
        let mut new_file_curr_offset = curr_offset;
        while curr_offset <= *file_size {
            let kv_byte_array = read_kv_bytes_from_file(file_ref, curr_offset)?;
            let kv_size = kv_byte_array.len() as u64;
            if reversed_index_map.contains_key(&curr_offset) {
                // insert key value
                new_file.seek(SeekFrom::Start(new_file_curr_offset))?;
                new_file.write_all(&kv_byte_array)?;

                // update index
                new_file.seek(SeekFrom::Start(reversed_index_map[&curr_offset]))?;
                new_file.write_all(&new_file_curr_offset.to_be_bytes())?;
                new_file_curr_offset += kv_size;
            }

            curr_offset += kv_size;
        }

        buffers.clear();
        *file = new_file;
        *file_size = new_file_curr_offset;

        fs::remove_file(&self.file_path)?;
        fs::rename(&new_file_path, &self.file_path)?;

        Ok(())
    }

    /// Returns the Some(Value) at the given address if the key there corresponds to the given key
    /// Otherwise, it returns None
    /// This is to handle hash collisions.
    pub(crate) fn get_value(&mut self, address: u64, key: &[u8]) -> io::Result<Option<Value>> {
        let mut buffers = acquire_lock!(self.buffers)?;

        // loop in reverse, starting at the back
        // since the latest buffers are the ones updated when new changes occur
        for buf in buffers.iter_mut().rev() {
            if buf.contains(address) {
                return buf.get_value(address, key);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        let bytes_read = file.read(&mut buf)?;

        // update buffers only upto actual data read (cater for partially filled buffer)
        buffers.push_back(Buffer::new(address, &buf[..bytes_read], self.buffer_size));

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = if entry.key == key {
            Some(Value::from(&entry))
        } else {
            None
        };

        Ok(value)
    }

    /// Checks to see if the given address is for the given key
    pub(crate) fn addr_belongs_to_key(&mut self, address: u64, key: &[u8]) -> io::Result<bool> {
        let mut buffers = acquire_lock!(self.buffers)?;
        // loop in reverse, starting at the back
        // since the latest buffers are the ones updated when new changes occur
        for buf in buffers.iter_mut().rev() {
            if buf.contains(address) {
                return buf.addr_belongs_to_key(address, key);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        let bytes_read = file.read(&mut buf)?;

        // update buffers only upto actual data read (cater for partially filled buffer)
        buffers.push_back(Buffer::new(address, &buf[..bytes_read], self.buffer_size));

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = entry.key == key;
        Ok(value)
    }

    /// Reads an arbitrary array at the given address and of given size and returns it
    pub(crate) fn read_at(&mut self, address: u64, size: usize) -> io::Result<Vec<u8>> {
        let mut buffers = acquire_lock!(self.buffers)?;
        // loop in reverse, starting at the back
        // since the latest buffers are the ones updated when new changes occur
        for buf in buffers.iter_mut().rev() {
            if buf.contains(address) {
                return buf.read_at(address, size);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = vec![0; self.buffer_size];
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        let bytes_read = file.read(&mut buf)?;

        // update buffers only upto actual data read (cater for partially filled buffer)
        buffers.push_back(Buffer::new(address, &buf[..bytes_read], self.buffer_size));

        let data_array = buf[0..size].to_vec();
        Ok(data_array)
    }

    /// Checks if the given range is within bounds for this buffer
    /// This is just a helper
    fn validate_bounds(&self, lower: u64, upper: u64, file_size: u64) -> io::Result<()> {
        if lower >= file_size || upper > file_size {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Span {}-{} is out of bounds for file size {}",
                    lower, upper, file_size
                ),
            ))
        } else {
            Ok(())
        }
    }
}

impl PartialEq for BufferPool {
    fn eq(&self, other: &Self) -> bool {
        let buffers = acquire_lock!(self.buffers).expect("lock acquired for self buffers");
        let file_size = acquire_lock!(self.file_size).expect("acquire lock on self file_size");
        let other_buffers = acquire_lock!(other.buffers).expect("lock acquired for other buffers");
        let other_file_size =
            acquire_lock!(other.file_size).expect("acquire lock on other file_size");

        self.capacity == other.capacity
            && self.buffer_size == other.buffer_size
            && self.max_keys == other.max_keys
            && self.redundant_blocks == other.redundant_blocks
            && self.file_path == other.file_path
            && *file_size == *other_file_size
            && *buffers == *other_buffers
    }

    fn ne(&self, other: &Self) -> bool {
        !self.eq(other)
    }
}

/// Initializes a new database file, giving it the header and the index place holders
fn initialize_db_file(
    file: &mut File,
    header: &DbFileHeader,
    index_bytes: Option<Vec<u8>>,
) -> io::Result<()> {
    let header_bytes = header.as_bytes();
    debug_assert_eq!(header_bytes.len(), 100);

    let index_block_bytes = match index_bytes {
        None => header.create_empty_index_blocks_bytes(),
        Some(v) => v,
    };

    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header_bytes)?;
    file.write_all(&index_block_bytes)?;
    file.seek(SeekFrom::Start(0))?;

    Ok(())
}

/// Re-initializes the database file, giving it the header and the index place holders
/// and truncating it. It returns the new file size
fn reinitialize_db_file(file: &mut File, header: &DbFileHeader) -> io::Result<u64> {
    let header_bytes = header.as_bytes();
    debug_assert_eq!(header_bytes.len(), 100);

    let index_block_bytes = header.create_empty_index_blocks_bytes();

    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header_bytes)?;
    file.write_all(&index_block_bytes)?;
    let size = header_bytes.len() as u64 + index_block_bytes.len() as u64;
    file.set_len(size)?;

    Ok(size)
}

/// Extracts the key value entry's bytes array from the file given the address where to find it
fn read_kv_bytes_from_file(file: &mut File, address: u64) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(address))?;
    let mut size_bytes: [u8; 4] = [0; 4];
    file.read(&mut size_bytes)?;
    let size = u32::from_be_bytes(size_bytes);
    let mut data = Vec::with_capacity(size as usize);
    file.seek(SeekFrom::Start(address))?;
    file.read(&mut data)?;
    Ok(data)
}

/// Extracts the index as a byte array
fn get_index_as_byte_array(file: &mut File, header: &DbFileHeader) -> io::Result<Vec<u8>> {
    let size = header.net_block_size * header.number_of_index_blocks;
    let mut data = Vec::with_capacity(size as usize);
    file.seek(SeekFrom::Start(100))?;
    file.read(&mut data)?;
    Ok(data)
}

/// Extracts an index map that has keys as the entry offset and
/// values as the index offset for only non-zero entry offsets
fn get_index_as_reversed_map(index_bytes: &Vec<u8>) -> io::Result<HashMap<u64, u64>> {
    let bytes_length = index_bytes.len();
    let map_size = bytes_length / 8;
    let mut map: HashMap<u64, u64> = HashMap::with_capacity(map_size);
    let mut i = 0;
    while i < bytes_length {
        let entry_offset = u64::from_be_bytes(slice_to_array(&index_bytes[i..i + 8])?);
        if entry_offset > 0 {
            // only non-zero entries are picked because zero signifies deleted or not yet inserted
            map.insert(entry_offset, 100 + i as u64);
        }

        i += 8;
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let test_data: Vec<(Config, BufferPool)> = vec![
            (
                (None, &Path::new(file_name), None, None, None),
                BufferPool {
                    capacity: DEFAULT_POOL_CAPACITY,
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: None,
                    buffers: Mutex::new(VecDeque::with_capacity(DEFAULT_POOL_CAPACITY)),
                    file: Mutex::new(
                        OpenOptions::new()
                            .write(true)
                            .read(true)
                            .create(true)
                            .open(file_name)
                            .expect("creates or open file"),
                    ),
                    file_path: Path::new(file_name).into(),
                    file_size: Mutex::new(DbFileHeader::new(None, None).key_values_start_point),
                },
            ),
            (
                (Some(60), &Path::new(file_name), None, None, None),
                BufferPool {
                    capacity: 60,
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: None,
                    buffers: Mutex::new(VecDeque::with_capacity(60)),
                    file: Mutex::new(
                        OpenOptions::new()
                            .write(true)
                            .read(true)
                            .create(true)
                            .open(file_name)
                            .expect("creates or open file"),
                    ),
                    file_path: Path::new(file_name).into(),
                    file_size: Mutex::new(DbFileHeader::new(None, None).key_values_start_point),
                },
            ),
            (
                (None, &Path::new(file_name), Some(360), None, None),
                BufferPool {
                    capacity: DEFAULT_POOL_CAPACITY,
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: Some(360),
                    redundant_blocks: None,
                    buffers: Mutex::new(VecDeque::with_capacity(DEFAULT_POOL_CAPACITY)),
                    file: Mutex::new(
                        OpenOptions::new()
                            .write(true)
                            .read(true)
                            .create(true)
                            .open(file_name)
                            .expect("creates or open file"),
                    ),
                    file_path: Path::new(file_name).into(),
                    file_size: Mutex::new(
                        DbFileHeader::new(Some(360), None).key_values_start_point,
                    ),
                },
            ),
            (
                (None, &Path::new(file_name), None, Some(4), None),
                BufferPool {
                    capacity: DEFAULT_POOL_CAPACITY,
                    buffer_size: get_vm_page_size() as usize,
                    max_keys: None,
                    redundant_blocks: Some(4),
                    buffers: Mutex::new(VecDeque::with_capacity(DEFAULT_POOL_CAPACITY)),
                    file: Mutex::new(
                        OpenOptions::new()
                            .write(true)
                            .read(true)
                            .create(true)
                            .open(file_name)
                            .expect("creates or open file"),
                    ),
                    file_path: Path::new(file_name).into(),
                    file_size: Mutex::new(DbFileHeader::new(None, Some(4)).key_values_start_point),
                },
            ),
            (
                (None, &Path::new(file_name), None, None, Some(2048)),
                BufferPool {
                    capacity: DEFAULT_POOL_CAPACITY,
                    buffer_size: 2048,
                    max_keys: None,
                    redundant_blocks: None,
                    buffers: Mutex::new(VecDeque::with_capacity(DEFAULT_POOL_CAPACITY)),
                    file: Mutex::new(
                        OpenOptions::new()
                            .write(true)
                            .read(true)
                            .create(true)
                            .open(file_name)
                            .expect("creates or open file"),
                    ),
                    file_path: Path::new(file_name).into(),
                    file_size: Mutex::new(DbFileHeader::new(None, None).key_values_start_point),
                },
            ),
        ];

        // delete the file so that BufferPool::new() can reinitialize it.
        fs::remove_file(&file_name).expect(&format!("delete file {:}", &file_name));

        for ((capacity, file_path, max_keys, redundant_blocks, buffer_size), expected) in test_data
        {
            let got = BufferPool::new(capacity, file_path, max_keys, redundant_blocks, buffer_size)
                .expect("new buffer pool");
            assert_eq!(&got, &expected);
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
        let test_data: Vec<Config> = vec![
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
        append_buffers(
            &mut pool,
            &[(initial_offset, &initial_data[..]), (0, &header_array[..])][..],
        );

        pool.append(&mut data).expect("appends data to buffer pool");

        let (data_in_file, bytes_read) =
            read_from_file(file_name, initial_offset + initial_data_length, data_length);
        let actual_file_size = get_actual_file_size(file_name);
        let final_file_size = get_pool_file_size(&mut pool);

        let mut buffers = acquire_lock!(pool.buffers).expect("acquire lock on buffers");
        let first_buf = buffers.pop_front().expect("buffer popped front");

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
    fn replace_in_file() {
        let file_name = "testdb.scdb";
        let data = &[72u8, 97, 108, 108, 101, 108, 117, 106, 97, 104];
        let data_length = data.len();
        let new_data = &[70u8, 94, 118, 10, 201, 108, 117, 146, 37, 154];
        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");
        let offset = get_pool_file_size(&mut pool);
        write_to_file(file_name, offset, data);
        increment_pool_file_size(&mut pool, data_length as u64);

        pool.replace(offset, &mut new_data.to_vec())
            .expect("replace data");

        let final_file_size = get_pool_file_size(&mut pool);
        let (data_in_file, bytes_read) = read_from_file(file_name, offset, data_length);
        let actual_file_size = get_actual_file_size(file_name);

        assert_eq!(final_file_size, offset + data_length as u64);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, data_length);
        assert_eq!(data_in_file, new_data);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    fn replace_in_pre_existing_buffer() {
        let file_name = "testdb.scdb";
        let initial_data = &[76u8, 67, 56];
        let initial_data_length = initial_data.len() as u64;
        let mut new_data = vec![72u8, 97, 108];
        let new_data_length = new_data.len();

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let initial_offset = get_actual_file_size(file_name);
        write_to_file(file_name, initial_offset, initial_data);
        increment_pool_file_size(&mut pool, initial_data_length);
        let (header_array, _) = read_from_file(file_name, 0, 100);
        let initial_file_size = get_pool_file_size(&mut pool);
        append_buffers(
            &mut pool,
            &[(initial_offset, &initial_data[..]), (0, &header_array[..])][..],
        );

        pool.replace(initial_offset, &mut new_data)
            .expect("replaces data in buffer");

        let (data_in_file, bytes_read) = read_from_file(file_name, initial_offset, new_data_length);
        let actual_file_size = get_actual_file_size(file_name);
        let final_file_size = get_pool_file_size(&mut pool);

        let mut buffers = acquire_lock!(pool.buffers).expect("acquire lock on buffers");
        let first_buf = buffers.pop_front().expect("buffer popped front");

        // assert things in file
        assert_eq!(final_file_size, initial_file_size);
        assert_eq!(final_file_size, actual_file_size);
        assert_eq!(bytes_read, new_data_length);
        assert_eq!(data_in_file, new_data);

        // assert things in buffer
        assert_eq!(first_buf.right_offset, final_file_size);
        assert_eq!(first_buf.data, new_data);

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    fn replace_out_of_bounds() {
        let file_name = "testdb.scdb";
        let initial_data = &[76u8, 67, 56];
        let initial_data_length = initial_data.len() as u64;
        let mut new_data = vec![72u8, 97, 108];

        let mut pool = BufferPool::new(None, &Path::new(file_name), None, None, None)
            .expect("new buffer pool");

        let initial_offset = get_actual_file_size(file_name);
        write_to_file(file_name, initial_offset, initial_data);
        increment_pool_file_size(&mut pool, initial_data_length);
        let (header_array, _) = read_from_file(file_name, 0, 100);
        append_buffers(
            &mut pool,
            &[(initial_offset, &initial_data[..]), (0, &header_array[..])][..],
        );

        let extra_offsets = &[3, 50, 78];

        for extra_offset in extra_offsets {
            let response = pool.replace(initial_offset + extra_offset, &mut new_data);
            // Not so sure though as this might write to file. Maybe it should throw an error as this is replacement
            // not appending
            assert!(response.is_err());
        }

        fs::remove_file(&file_name).expect(&format!("delete file {}", &file_name));
    }

    #[test]
    fn clear_file_works() {
        todo!()
    }

    #[test]
    fn compact_file_works() {
        todo!()
    }

    #[test]
    fn get_value_works() {
        todo!()
    }

    #[test]
    fn get_value_out_of_bounds() {
        todo!()
    }

    #[test]
    fn addr_belongs_to_key_works() {
        todo!()
    }

    #[test]
    fn addr_belongs_to_key_works_out_of_bounds() {
        todo!()
    }

    #[test]
    fn read_at_works() {
        todo!()
    }

    #[test]
    fn read_at_works_out_of_bounds() {
        todo!()
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
        let file_size = acquire_lock!(pool.file_size).expect("get lock on file size");
        let initial_file_size = *file_size;
        initial_file_size
    }

    /// Manually increments the pool's file_size attribute
    fn increment_pool_file_size(pool: &mut BufferPool, incr: u64) {
        let mut file_size = acquire_lock!(pool.file_size).expect("get lock on file size");
        *file_size += incr
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

    /// Creates and appends buffers to the pool from the offset-data pairs
    fn append_buffers(pool: &mut BufferPool, pairs: &[(u64, &[u8])]) {
        let mut buffers = acquire_lock!(pool.buffers).expect("acquire lock on buffers");

        for (offset, data) in pairs {
            buffers.push_back(Buffer::new(*offset, data, pool.buffer_size));
        }
    }
}
