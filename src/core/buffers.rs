use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::core::utils::get_vm_page_size;
use crate::core::{DbFileHeader, KeyValueEntry};

pub(crate) struct BufferPool {
    capacity: usize,
    buffer_size: usize,
    // These are used only for reads
    buffers: VecDeque<Buffer>,
    file: File,
    file_path: PathBuf,
    file_size: u64,
}

pub(crate) struct Buffer {
    data: Vec<u8>,
    left_offset: u64,
    right_offset: u64,
}

impl Buffer {
    /// Creates a new Buffer with the given left_offset
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
    fn append(&mut self, data: &mut Vec<u8>) {
        let data_length = data.len();
        self.data.append(data);
        self.right_offset += data_length as u64;
    }

    /// Replaces the data at the given address with the new data
    fn replace(&mut self, address: u64, data: Vec<u8>) {
        let data_length = data.len();
        let start = address as usize;
        let stop = start + data_length;
        self.data = self.data.splice(start..stop, data).collect();
    }
}

impl BufferPool {
    /// Creates a new BufferPool with the given `capacity` number of Buffers and
    /// for the file at the given path (creating it if necessary)
    pub(crate) fn new(
        capacity: usize,
        file_path: &Path,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        buffer_size: Option<usize>,
    ) -> io::Result<Self> {
        let buffer_size = match buffer_size {
            None => get_vm_page_size() as usize,
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
            initialize_db_file(&mut file, &header)?;
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
    pub(crate) fn append(&mut self, data: &mut Vec<u8>) -> io::Result<()> {
        for mut buf in &self.buffers {
            if buf.contains(self.file_size) {
                buf.append(data);
                self.file.seek(SeekFrom::End(0))?;
                self.file.write_all(data)?;
                return Ok(());
            }
        }

        let start = self.file.seek(SeekFrom::End(0))?;
        let new_file_size = start + data.len() as u64;
        self.file.write_all(data)?;
        self.file_size = new_file_size;

        Ok(())
    }

    /// Inserts a given data array at the given address. Do note that this overwrites
    /// the existing data at that address. If you are looking to update to a value that
    /// could have a different length from the previous one, append it to the bottom
    /// then overwrite the previous offset in the index with the offset of the new entry
    pub(crate) fn replace(&mut self, address: u64, data: &[u8]) -> io::Result<()> {
        for mut buf in &self.buffers {
            if buf.contains(address) {
                buf.replace(address, data.to_vec());
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
            .open(new_file_path)?;

        let header: DbFileHeader = DbFileHeader::from_file(&self.file)?;
        let index_bytes_array = get_index_as_byte_array(&self.file, &header)?;
        let reversed_index_map: HashMap<u64, u64> = get_index_as_reversed_map(self.file, &header)?;
        initialize_db_file(&mut new_file, &header, Some(index_bytes_array))?;

        let mut curr_offset = header.key_values_start_point;
        let mut new_file_curr_offset = curr_offset;
        while curr_offset <= self.file_size {
            let kv_entry: KeyValueEntry = KeyValueEntry::from_file(&self.file, curr_offset)?;
            if reversed_index_map.contains_key(&curr_offset) {
                new_file.seek(SeekFrom::Start(new_file_curr_offset))?;
                new_file.write_all(kv_entry.as_bytes())?;
                new_file.seek(SeekFrom::Start(index_bytes_array[curr_offset]))?;
                new_file.write_all(&new_file_curr_offset.to_be_bytes())?;
                new_file_curr_offset += kv_entry.get_size_in_bits();
            }

            curr_offset += kv_entry.get_size_in_bits();
        }

        self.buffers.clear();
        self.file = new_file;
        self.file_size = new_file_curr_offset;

        fs::remove_file(&self.file_path)?;
        fs::rename(&new_file_path, &self.file_path)?;

        Ok(())
    }

    /// Returns the value at the given address, of the given size in bytes
    pub(crate) fn get_key_value(&mut self, address: u64) -> io::Result<KeyValueEntry> {
        for mut buf in &self.buffers {
            if buf.contains(address) {
                buf.replace(address, data.to_vec());
                return Ok(buf.get_key_value(address));
            }
        }

        let entry: KeyValueEntry = KeyValueEntry::from_file(&self.file, address)?;

        // update the buffer for caching
        if self.buffers.len() >= self.capacity {
            self.buffers.pop_front();
        }
        let mut buffer_array: Vec<u8> = entry.as_bytes();
        self.buffers.push_back(Buffer::new(address, buffer_array));
        Ok(entry)
    }
}

/// Initializes a new database file, giving it the header and the index place holders
fn initialize_db_file(
    file: &mut File,
    header: &DbFileHeader,
    index_bytes: Option<Vec<u8>>,
) -> io::Result<()> {
    let header_bytes = header.get_header_as_bytes();
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
