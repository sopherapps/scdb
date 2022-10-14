use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Seek, SeekFrom};

use crate::internal::utils;

const KEY_VALUE_MIN_SIZE_IN_BYTES: u32 = 4 + 4 + 8 + 1;

pub(crate) struct DbFileHeader {
    pub(crate) title: String,
    pub(crate) block_size: u32,
    pub(crate) max_keys: u64,
    pub(crate) redundant_blocks: u16,
    pub(crate) last_offset: u64,
    pub(crate) items_per_index_block: u64,
    pub(crate) number_of_index_blocks: u64,
    pub(crate) key_values_start_point: u64,
    pub(crate) net_block_size: u64,
}

pub(crate) struct KeyValueEntry<'a> {
    pub(crate) size: u32,
    pub(crate) key_size: u32,
    pub(crate) key: &'a [u8],
    pub(crate) expiry: u64,
    pub(crate) deleted: bool,
    pub(crate) value: &'a [u8],
}

impl DbFileHeader {
    /// Creates a new DbFileHeader
    pub(crate) fn new(max_keys: Option<u64>, redundant_blocks: Option<u16>) -> Self {
        let max_keys = match max_keys {
            None => 1_000_000,
            Some(v) => v,
        };
        let redundant_blocks = match redundant_blocks {
            None => 1,
            Some(v) => v,
        };
        let block_size = utils::get_vm_page_size();
        let mut header = Self {
            title: "Scdb versn 0.001".to_string(),
            block_size,
            max_keys,
            redundant_blocks,
            last_offset: 0,
            items_per_index_block: 0,
            number_of_index_blocks: 0,
            key_values_start_point: 0,
            net_block_size: 0,
        };

        header.update_derived_props();
        header
    }

    /// Computes the properties that depend on the user-defined/default properties and update them
    /// on self
    fn update_derived_props(&mut self) {
        self.items_per_index_block = (self.block_size as f64 / 8.0).floor() as u64;
        self.number_of_index_blocks = (self.max_keys as f64 / self.items_per_index_block as f64)
            as u64
            + self.redundant_blocks as u64;
        self.last_offset = 100 + (self.items_per_index_block * 8 * self.number_of_index_blocks);
        self.key_values_start_point = self.last_offset;
        self.net_block_size = self.items_per_index_block * 8;
    }

    /// Retrieves the byte array that represents the header.
    pub(crate) fn get_header_as_bytes(&self) -> Vec<u8> {
        self.title
            .as_bytes()
            .iter()
            .chain(&self.block_size.to_be_bytes())
            .chain(&self.max_keys.to_be_bytes())
            .chain(&self.redundant_blocks.to_be_bytes())
            .chain(&self.last_offset.to_be_bytes())
            .map(|v| v.to_owned())
            .collect()
    }

    /// Creates a place holder for the index blocks.
    pub(crate) fn create_empty_index_blocks_bytes(&self) -> Vec<u8> {
        // each index entry is 8 bytes
        let length = self.number_of_index_blocks * self.items_per_index_block * 8;
        vec![0; length as usize]
    }

    /// Extracts the header from the data array
    pub(crate) fn from_data_array(data: &[u8]) -> io::Result<Self> {
        let title = String::from_utf8(data[0..16].to_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let block_size = u32::from_be_bytes(extract_array::<4>(&data[16..20])?);
        let max_keys = u64::from_be_bytes(extract_array::<8>(&data[20..28])?);
        let redundant_blocks = u16::from_be_bytes(extract_array::<2>(&data[28..30])?);
        let last_offset = u64::from_be_bytes(extract_array::<8>(&data[30..38])?);

        let mut header = Self {
            title,
            block_size,
            max_keys,
            redundant_blocks,
            last_offset,
            items_per_index_block: 0,
            number_of_index_blocks: 0,
            key_values_start_point: 0,
            net_block_size: 0,
        };

        header.update_derived_props();
        Ok(header)
    }

    /// Extracts the header from a database file
    pub(crate) fn from_file(file: &mut File) -> io::Result<Self> {
        file.seek(SeekFrom::Start(0))?;
        let mut buf: [u8; 100];
        file.read(&mut buf)?;
        Self::from_data_array(&buf)
    }
}

impl<'a> KeyValueEntry<'a> {
    /// Creates a new KeyValueEntry
    pub(crate) fn new(key: &'a [u8], value: &'a [u8], expiry: u64) -> Self {
        let key_size = key.len() as u32;
        let size = key_size + KEY_VALUE_MIN_SIZE_IN_BYTES + value.len() as u32;

        Self {
            size,
            key_size,
            key,
            expiry,
            deleted: false,
            value,
        }
    }

    /// Extracts the key value entry from the data array
    pub(crate) fn from_data_array(data: &'a [u8], offset: usize) -> io::Result<Self> {
        let mut cursor = offset;
        let size = u32::from_be_bytes(extract_array(&data[cursor..4])?);
        cursor += 4;
        let key_size = u32::from_be_bytes(extract_array(&data[cursor..4])?);
        cursor += 4;
        let key = &data[cursor..key_size as usize];
        cursor += key_size as usize;
        let expiry = u64::from_be_bytes(extract_array(&data[cursor..8])?);
        cursor += 8;
        let deleted = u8::from_be_bytes(extract_array(&data[cursor..1])?);
        cursor += 1;
        let value_size = (size - key_size - KEY_VALUE_MIN_SIZE_IN_BYTES) as usize;
        let value = &data[cursor..value_size];

        let entry = Self {
            size,
            key_size,
            key,
            expiry,
            deleted: if deleted == 1 { true } else { false },
            value,
        };
        Ok(entry)
    }
}

/// Extracts a byte array of size N from a byte array slice
pub(crate) fn extract_array<const N: usize>(data: &[u8]) -> io::Result<[u8; N]> {
    data.try_into()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Extracts the key value entry's bytes array from the file given the address where to find it
pub(crate) fn read_kv_bytes_from_file(file: &mut File, address: u64) -> io::Result<Vec<u8>> {
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
pub(crate) fn get_index_as_byte_array(
    file: &mut File,
    header: &DbFileHeader,
) -> io::Result<Vec<u8>> {
    let size = header.net_block_size * header.number_of_index_blocks;
    let mut data = Vec::with_capacity(size as usize);
    // FIXME: Consider using BufferReader
    file.seek(SeekFrom::Start(100))?;
    file.read(&mut data)?;
    Ok(data)
}

/// Extracts an index map that has keys as the entry offset and
/// values as the index offset for only non-zero entry offsets
pub(crate) fn get_index_as_reversed_map(index_bytes: &Vec<u8>) -> io::Result<HashMap<u64, u64>> {
    let bytes_length = index_bytes.len();
    let map_size = bytes_length / 8;
    let mut map: HashMap<u64, u64> = HashMap::with_capacity(map_size);
    let mut i = 0;
    while i < bytes_length {
        let entry_offset = u64::from_be_bytes(extract_array(&index_bytes[i..i + 8])?);
        if entry_offset > 0 {
            // only non-zero entries are picked because zero signifies deleted or not yet inserted
            map[&entry_offset] = 100 + i as u64;
        }

        i += 8;
    }

    Ok(map)
}
