use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::internal::entries::{
    get_index_as_byte_array, get_index_as_reversed_map, read_kv_bytes_from_file,
};
use crate::internal::utils::get_vm_page_size;
use crate::internal::{DbFileHeader, KeyValueEntry};

const DEFAULT_POOL_CAPACITY: usize = 5;

pub(crate) struct BufferPool {
    capacity: usize,
    buffer_size: usize,
    // These are used only for reads
    buffers: VecDeque<Buffer>,
    pub(crate) file: File,
    file_path: PathBuf,
    pub(crate) file_size: u64,
}

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
            buffers: VecDeque::with_capacity(capacity),
            file,
            file_size,
            file_path: file_path.into(),
        };

        Ok(v)
    }

    /// Appends a given data array to the file attached to this buffer pool
    /// It returns the address where the data was appended
    pub(crate) fn append(&mut self, data: &mut Vec<u8>) -> io::Result<u64> {
        for i in 0..self.buffers.len() {
            if self.buffers[i].contains(self.file_size) {
                let addr = self.buffers[i].append(data);
                self.file_size = self.buffers[i].left_offset;
                self.file.seek(SeekFrom::End(0))?;
                self.file.write_all(data)?;
                self.update_file_last_offset()?;
                return Ok(addr);
            }
        }

        let start = self.file.seek(SeekFrom::End(0))?;
        let new_file_size = start + data.len() as u64;
        self.file.write_all(data)?;
        self.file_size = new_file_size;
        self.update_file_last_offset()?;

        Ok(start)
    }

    /// Updates the last_offset value in the Header of the database file
    /// from the self.file_size value on self
    fn update_file_last_offset(&mut self) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(30))?;
        self.file.write_all(&self.file_size.to_be_bytes())?;
        Ok(())
    }

    /// Inserts a given data array at the given address. Do note that this overwrites
    /// the existing data at that address. If you are looking to update to a value that
    /// could have a different length from the previous one, append it to the bottom
    /// then overwrite the previous offset in the index with the offset of the new entry
    pub(crate) fn replace(&mut self, address: u64, data: &[u8]) -> io::Result<()> {
        for i in 0..self.buffers.len() {
            if self.buffers[i].contains(address) {
                self.buffers[i].replace(address, data.to_vec());
                self.file.seek(SeekFrom::Start(address))?;
                self.file.write_all(data)?;
                return Ok(());
            }
        }

        self.file.seek(SeekFrom::Start(address))?;
        self.file.write_all(data)?;

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

        let header: DbFileHeader = DbFileHeader::from_file(&mut self.file)?;
        let index_bytes_array = get_index_as_byte_array(&mut self.file, &header)?;
        let reversed_index_map: HashMap<u64, u64> = get_index_as_reversed_map(&index_bytes_array)?;
        initialize_db_file(&mut new_file, &header, Some(index_bytes_array))?;

        let mut curr_offset = header.key_values_start_point;
        let mut new_file_curr_offset = curr_offset;
        while curr_offset <= self.file_size {
            let kv_byte_array = read_kv_bytes_from_file(&mut self.file, curr_offset)?;
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

        self.buffers.clear();
        self.file = new_file;
        self.file_size = new_file_curr_offset;

        fs::remove_file(&self.file_path)?;
        fs::rename(&new_file_path, &self.file_path)?;

        Ok(())
    }

    /// Returns the Some(Value) at the given address if the key there corresponds to the given key
    /// Otherwise, it returns None
    /// This is to handle hash collisions.
    pub(crate) fn get_value(&mut self, address: u64, key: &[u8]) -> io::Result<Option<Vec<u8>>> {
        for buf in &self.buffers {
            if buf.contains(address) {
                return buf.get_value(address, key);
            }
        }

        if self.buffers.len() >= self.capacity {
            self.buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        self.file.seek(SeekFrom::Start(address))?;
        self.file.read(&mut buf)?;

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = if entry.key == key {
            Some(entry.value.to_vec())
        } else {
            None
        };

        // update buffers
        self.buffers.push_back(Buffer::new(address, buf));

        Ok(value)
    }

    /// Checks to see if the given address is for the given key
    pub(crate) fn addr_belongs_to_key(&mut self, address: u64, key: &[u8]) -> io::Result<bool> {
        for buf in &self.buffers {
            if buf.contains(address) {
                return buf.addr_belongs_to_key(address, key);
            }
        }

        if self.buffers.len() >= self.capacity {
            self.buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        self.file.seek(SeekFrom::Start(address))?;
        self.file.read(&mut buf)?;

        let entry: KeyValueEntry = KeyValueEntry::from_data_array(&buf, 0)?;

        let value = entry.key == key;

        // update buffers
        self.buffers.push_back(Buffer::new(address, buf));

        Ok(value)
    }

    /// Reads an arbitrary array at the given address and of given size and returns it
    pub(crate) fn read_at(&mut self, address: u64, size: usize) -> io::Result<Vec<u8>> {
        for i in 0..self.buffers.len() {
            if self.buffers[i].contains(address) {
                return self.buffers[i].read_at(address, size);
            }
        }

        if self.buffers.len() >= self.capacity {
            self.buffers.pop_front();
        }

        let mut buf: Vec<u8> = Vec::with_capacity(self.buffer_size);
        self.file.seek(SeekFrom::Start(address))?;
        self.file.read(&mut buf)?;

        let data_array = buf[0..size].to_vec();

        // update buffers
        self.buffers.push_back(Buffer::new(address, buf));

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
    fn get_value(&self, address: u64, key: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let offset = (address - self.left_offset) as usize;
        let entry = KeyValueEntry::from_data_array(&self.data, offset)?;
        let value = if entry.key == key {
            Some(entry.value.to_vec())
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
