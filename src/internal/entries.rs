use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};

use crate::internal::utils::get_current_timestamp;
use crate::internal::{get_hash, utils};

const KEY_VALUE_MIN_SIZE_IN_BYTES: u32 = 4 + 4 + 8;
pub(crate) const INDEX_ENTRY_SIZE_IN_BYTES: u64 = 8;
const HEADER_SIZE_IN_BYTES: u64 = 100;

#[derive(Debug, PartialEq)]
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

#[derive(Debug, PartialEq)]
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
        let block_size = u32::from_be_bytes(utils::slice_to_array::<4>(&data[16..20])?);
        let max_keys = u64::from_be_bytes(utils::slice_to_array::<8>(&data[20..28])?);
        let redundant_blocks = u16::from_be_bytes(utils::slice_to_array::<2>(&data[28..30])?);

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
    /// `key` is the byte array of the key
    /// `value` is the byte array of the value
    /// `expiry` is the timestamp (in seconds from unix epoch)
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
        let size = u32::from_be_bytes(utils::slice_to_array(&data[offset..offset + 4])?);
        let key_size = u32::from_be_bytes(utils::slice_to_array(&data[offset + 4..offset + 8])?);
        let k_size = key_size as usize;
        let key = &data[offset + 8..offset + 8 + k_size];
        let expiry = u64::from_be_bytes(utils::slice_to_array(
            &data[offset + 8 + k_size..offset + k_size + 16],
        )?);
        let value_size = (size - key_size - KEY_VALUE_MIN_SIZE_IN_BYTES) as usize;
        let value = &data[offset + k_size + 16..offset + k_size + 16 + value_size];

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

#[cfg(test)]
mod tests {
    use super::*;

    const KV_DATA_ARRAY: [u8; 22] = [
        /* size: 22u32*/ 0u8, 0, 0, 22, /* key size: 3u32*/ 0, 0, 0, 3,
        /* key */ 102, 111, 111, /* expiry 0u64 */ 0, 0, 0, 0, 0, 0, 0, 0,
        /* value */ 98, 97, 114,
    ];

    #[test]
    fn key_value_entry_from_data_array() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let got = KeyValueEntry::from_data_array(&KV_DATA_ARRAY[..], 0)
            .expect("key value from data array");
        assert_eq!(&got, &kv, "got = {:?}, expected = {:?}", &got, &kv);
    }

    #[test]
    fn key_value_entry_from_data_array_with_offset() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let data_array: Vec<u8> = [89u8, 78u8]
            .iter()
            .chain(&KV_DATA_ARRAY)
            .map(|v| v.to_owned())
            .collect();
        let got =
            KeyValueEntry::from_data_array(&data_array[..], 2).expect("key value from data array");
        assert_eq!(&got, &kv, "got = {:?}, expected = {:?}", &got, &kv);
    }

    #[test]
    fn key_value_as_bytes() {
        let kv = KeyValueEntry::new(&b"foo"[..], &b"bar"[..], 0);
        let kv_vec = KV_DATA_ARRAY.to_vec();
        let got = kv.as_bytes();
        assert_eq!(&got, &kv_vec, "got = {:?}, expected = {:?}", &got, &kv_vec);
    }

    #[test]
    fn is_expired_works() {
        let never_expires = KeyValueEntry::new(&b"never_expires"[..], &b"bar"[..], 0);
        // 1666023836u64 is some past timestamp in October 2022
        let expired = KeyValueEntry::new(&b"expires"[..], &b"bar"[..], 1666023836u64);
        let not_expired = KeyValueEntry::new(
            &b"not_expired"[..],
            &b"bar"[..],
            get_current_timestamp() * 2,
        );

        assert!(!never_expires.is_expired());
        assert!(!not_expired.is_expired());
        assert!(expired.is_expired());
    }
}
