use crate::internal::entries::db_file_header::DbFileHeader;
use crate::internal::entries::key_value::KeyValueEntry;
use crate::internal::macros::acquire_lock;
use crate::internal::utils::{get_vm_page_size, slice_to_array};
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

const DEFAULT_POOL_CAPACITY: usize = 5;

#[derive(Debug, PartialEq)]
pub(crate) struct Value {
    pub(crate) data: Vec<u8>,
    pub(crate) is_expired: bool,
}

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

#[derive(Debug, PartialEq)]
pub(crate) struct Buffer {
    data: Vec<u8>,
    left_offset: u64,
    right_offset: u64,
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

        let should_create_new = file_path.exists();
        let mut file = OpenOptions::new()
            .append(true)
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

        for buf in &mut *buffers {
            if buf.contains(*file_size) {
                let addr = buf.append(data);
                *file_size = buf.left_offset;
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
        let mut file = acquire_lock!(self.file)?;
        let mut buffers = acquire_lock!(self.buffers)?;

        for buf in &mut *buffers {
            if buf.contains(address) {
                buf.replace(address, data.to_vec());
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
            .append(true)
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

        for buf in &mut *buffers {
            if buf.contains(address) {
                return buf.get_value(address, key);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        file.read(&mut buf)?;

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = if entry.key == key {
            Some(Value::from(&entry))
        } else {
            None
        };

        // update buffers
        buffers.push_back(Buffer::new(address, buf));

        Ok(value)
    }

    /// Checks to see if the given address is for the given key
    pub(crate) fn addr_belongs_to_key(&mut self, address: u64, key: &[u8]) -> io::Result<bool> {
        let mut buffers = acquire_lock!(self.buffers)?;
        for buf in &*buffers {
            if buf.contains(address) {
                return buf.addr_belongs_to_key(address, key);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        file.read(&mut buf)?;

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = entry.key == key;

        // update buffers
        buffers.push_back(Buffer::new(address, buf));

        Ok(value)
    }

    /// Reads an arbitrary array at the given address and of given size and returns it
    pub(crate) fn read_at(&mut self, address: u64, size: usize) -> io::Result<Vec<u8>> {
        let mut buffers = acquire_lock!(self.buffers)?;
        for buf in &mut *buffers {
            if buf.contains(address) {
                return buf.read_at(address, size);
            }
        }

        if buffers.len() >= self.capacity {
            buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        let mut file = acquire_lock!(self.file)?;
        file.seek(SeekFrom::Start(address))?;
        file.read(&mut buf)?;

        let data_array = buf[0..size].to_vec();

        // update buffers
        buffers.push_back(Buffer::new(address, buf));

        Ok(data_array)
    }
}

impl Buffer {
    /// Creates a new Buffer with the given left_offset
    #[inline]
    fn new(left_offset: u64, data: Vec<u8>) -> Self {
        let right_offset = left_offset + data.len() as u64;
        Self {
            data,
            left_offset,
            right_offset,
        }
    }
    /// Checks if the given address is in this buffer
    #[inline]
    fn contains(&self, address: u64) -> bool {
        self.left_offset <= address && address <= self.right_offset
    }

    /// Appends the data to the end of the array
    /// It returns the address (or offset) where the data was appended
    #[inline]
    fn append(&mut self, data: &mut Vec<u8>) -> u64 {
        let data_length = data.len();
        self.data.append(data);
        let prev_right_offset = self.right_offset;
        self.right_offset += data_length as u64;
        return prev_right_offset;
    }

    /// Replaces the data at the given address with the new data
    #[inline]
    fn replace(&mut self, address: u64, data: Vec<u8>) {
        let data_length = data.len();
        let start = address as usize;
        let stop = start + data_length;
        self.data = self.data.splice(start..stop, data).collect();
    }

    /// Returns the Some(Value) at the given address if the key there corresponds to the given key
    /// Otherwise, it returns None
    /// This is to handle hash collisions.
    #[inline]
    fn get_value(&self, address: u64, key: &[u8]) -> io::Result<Option<Value>> {
        let offset = (address - self.left_offset) as usize;
        let entry = KeyValueEntry::from_data_array(&self.data, offset)?;
        let value = if entry.key == key {
            Some(Value::from(&entry))
        } else {
            None
        };

        Ok(value)
    }

    /// Reads an arbitrary array at the given address and of given size and returns it
    #[inline]
    fn read_at(&mut self, address: u64, size: usize) -> io::Result<Vec<u8>> {
        let offset = (address - self.left_offset) as usize;
        let data_array = self.data[offset..size].to_vec();
        Ok(data_array)
    }

    /// Checks to see if the given address is for the given key
    #[inline]
    fn addr_belongs_to_key(&self, address: u64, key: &[u8]) -> io::Result<bool> {
        let offset = (address - self.left_offset) as usize;
        let entry = KeyValueEntry::from_data_array(&self.data, offset)?;
        Ok(entry.key == key)
    }
}

impl From<&KeyValueEntry<'_>> for Value {
    fn from(entry: &KeyValueEntry) -> Self {
        Self {
            data: entry.value.to_vec(),
            is_expired: entry.is_expired(),
        }
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
    use crate::internal::get_current_timestamp;

    #[test]
    fn value_from_key_value_entry() {
        type Record<'a> = (KeyValueEntry<'a>, Value);
        let test_table: Vec<Record> = vec![
            (
                KeyValueEntry::new(&b"never_expires"[..], &b"barer"[..], 0),
                Value {
                    data: vec![98, 97, 114, 101, 114],
                    is_expired: false,
                },
            ),
            (
                KeyValueEntry::new(&b"expires"[..], &b"Hallelujah"[..], 1666023836u64),
                Value {
                    data: vec![72, 97, 108, 108, 101, 108, 117, 106, 97, 104],
                    is_expired: true,
                },
            ),
            (
                KeyValueEntry::new(
                    &b"not_expired"[..],
                    &b"bar"[..],
                    get_current_timestamp() * 2,
                ),
                Value {
                    data: vec![98, 97, 114],
                    is_expired: false,
                },
            ),
        ];

        for (kv, expected) in test_table {
            assert_eq!(&Value::from(&kv), &expected);
        }
    }
}
