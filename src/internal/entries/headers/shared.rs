use crate::internal::get_hash;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};

pub(crate) const INDEX_ENTRY_SIZE_IN_BYTES: u64 = 8;

pub(crate) const HEADER_SIZE_IN_BYTES: u64 = 100;

pub(crate) trait Header: Sized {
    /// Gets the number of items per index block
    fn get_items_per_index_block(&self) -> u64;

    /// Gets the number of index blocks for given header
    fn get_number_of_index_blocks(&self) -> u64;

    /// Gets the net size of each index block
    fn get_net_block_size(&self) -> u64;

    /// Retrieves the byte array that represents the header.
    fn as_bytes(&self) -> Vec<u8>;

    /// Extracts the header from the data array
    fn from_data_array(data: &[u8]) -> io::Result<Self>;

    /// Extracts the header from a database file
    fn from_file(file: &mut File) -> io::Result<Self> {
        file.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; HEADER_SIZE_IN_BYTES as usize];
        let data_len = file.read(&mut buf)?;
        if data_len < HEADER_SIZE_IN_BYTES as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "data should be at least {} bytes in length",
                    HEADER_SIZE_IN_BYTES
                ),
            ));
        }

        Self::from_data_array(&buf)
    }

    /// Computes the offset for the given key in the first index block.
    /// It uses the meta data in this header
    /// i.e. number of items per block and the `INDEX_ENTRY_SIZE_IN_BYTES`
    fn get_index_offset(&self, key: &[u8]) -> u64 {
        let hash = get_hash(key, self.get_items_per_index_block());
        HEADER_SIZE_IN_BYTES + (hash * INDEX_ENTRY_SIZE_IN_BYTES)
    }

    /// Returns the index offset for the nth index block if `initial_offset` is the offset
    /// in the top most index block
    /// `n` starts at zero where zero is the top most index block
    fn get_index_offset_in_nth_block(&self, initial_offset: u64, n: u64) -> io::Result<u64> {
        let number_of_index_blocks = self.get_number_of_index_blocks();
        if n >= number_of_index_blocks {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "block {} out of bounds of {} blocks",
                    n, number_of_index_blocks
                ),
            ));
        }

        Ok(initial_offset + (self.get_net_block_size() * n))
    }
}

/// A struct containing the common properties derived
/// from other properties of headers
pub(crate) struct DerivedHeaderProps {
    pub(crate) items_per_index_block: u64,
    pub(crate) number_of_index_blocks: u64,
    pub(crate) values_start_point: u64,
    pub(crate) net_block_size: u64,
}

impl DerivedHeaderProps {
    /// creates a new DerivedHeaderProps basing on the block_size, max_keys and redundant_blocks
    pub(crate) fn new(block_size: u32, max_keys: u64, redundant_blocks: u16) -> Self {
        let items_per_index_block =
            (block_size as f64 / INDEX_ENTRY_SIZE_IN_BYTES as f64).floor() as u64;
        let number_of_index_blocks = (max_keys as f64 / items_per_index_block as f64).ceil() as u64
            + redundant_blocks as u64;
        let net_block_size = items_per_index_block * INDEX_ENTRY_SIZE_IN_BYTES;
        let values_start_point = HEADER_SIZE_IN_BYTES + (net_block_size * number_of_index_blocks);
        return Self {
            items_per_index_block,
            number_of_index_blocks,
            net_block_size,
            values_start_point,
        };
    }
}

pub const DEFAULT_DB_MAX_KEYS: u64 = 1_000_000;
pub const DEFAULT_DB_REDUNDANT_BLOCKS: u16 = 1;
