use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};

use crate::internal::utils::get_current_timestamp;
use crate::internal::{get_hash, utils};

const KEY_VALUE_MIN_SIZE_IN_BYTES: u32 = 4 + 4 + 8;
pub(crate) const INDEX_ENTRY_SIZE_IN_BYTES: u64 = 8;
const HEADER_SIZE_IN_BYTES: u64 = 100;

pub(crate) struct DbFileHeader {
    pub(crate) title: String,
    pub(crate) block_size: u32,
    pub(crate) max_keys: u64,
    pub(crate) redundant_blocks: u16,
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
        self.items_per_index_block =
            (self.block_size as f64 / INDEX_ENTRY_SIZE_IN_BYTES as f64).floor() as u64;
        self.number_of_index_blocks = (self.max_keys as f64 / self.items_per_index_block as f64)
            as u64
            + self.redundant_blocks as u64;
        self.key_values_start_point = HEADER_SIZE_IN_BYTES
            + (self.items_per_index_block
                * INDEX_ENTRY_SIZE_IN_BYTES
                * self.number_of_index_blocks);
        self.net_block_size = self.items_per_index_block * INDEX_ENTRY_SIZE_IN_BYTES;
    }

    /// Retrieves the byte array that represents the header.
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        self.title
            .as_bytes()
            .iter()
            .chain(&self.block_size.to_be_bytes())
            .chain(&self.max_keys.to_be_bytes())
            .chain(&self.redundant_blocks.to_be_bytes())
            .map(|v| v.to_owned())
            .collect()
    }

    /// Creates a place holder for the index blocks.
    pub(crate) fn create_empty_index_blocks_bytes(&self) -> Vec<u8> {
        // each index entry is 8 bytes
        let length =
            self.number_of_index_blocks * self.items_per_index_block * INDEX_ENTRY_SIZE_IN_BYTES;
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
        let mut buf = [0u8; HEADER_SIZE_IN_BYTES as usize];
        file.read(&mut buf)?;
        Self::from_data_array(&buf)
    }

    /// Computes the offset for the given key in the first index block.
    /// It uses the meta data in this header
    /// i.e. number of items per block and the `INDEX_ENTRY_SIZE_IN_BYTES`
    pub(crate) fn get_index_offset(&self, key: &[u8]) -> u64 {
        let hash = get_hash(key, self.items_per_index_block);
        HEADER_SIZE_IN_BYTES + (hash * INDEX_ENTRY_SIZE_IN_BYTES)
    }

    /// Returns the index offset for the nth index block if `initial_offset` is the offset
    /// in the top most index block
    /// `n` starts at zero where zero is the top most index block
    pub(crate) fn get_index_offset_in_nth_block(
        &self,
        initial_offset: u64,
        n: u64,
    ) -> io::Result<u64> {
        if n >= self.number_of_index_blocks {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "block {} out of bounds of {} blocks",
                    n, self.number_of_index_blocks
                ),
            ));
        }

        Ok(initial_offset + (self.net_block_size * n))
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
        let value_size = (size - key_size - KEY_VALUE_MIN_SIZE_IN_BYTES) as usize;
        let value = &data[cursor..value_size];

        let entry = Self {
            size,
            key_size,
            key,
            expiry,
            value,
        };
        Ok(entry)
    }

    /// Retrieves the byte array that represents the key value entry.
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
        self.size
            .to_be_bytes()
            .iter()
            .chain(&self.key_size.to_be_bytes())
            .chain(self.key)
            .chain(&self.expiry.to_be_bytes())
            .chain(self.value)
            .map(|v| v.to_owned())
            .collect()
    }

    /// Returns true if key has lived for longer than its time-to-live
    /// It will always return false if time-to-live was never set
    pub(crate) fn is_expired(&self) -> bool {
        if self.expiry == 0 {
            false
        } else {
            self.expiry < get_current_timestamp()
        }
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
            map.insert(entry_offset, 100 + i as u64);
        }

        i += 8;
    }

    Ok(map)
}
