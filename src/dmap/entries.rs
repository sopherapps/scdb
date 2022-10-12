use crate::dmap::{mmap, utils};

pub(crate) struct DbFileHeader {
    pub(crate) title: &'static [u8; 16],
    pub(crate) block_size: u32,
    pub(crate) max_keys: u64,
    pub(crate) redundant_blocks: u16,
    pub(crate) last_offset: u64,
    pub(crate) items_per_index_block: u64,
    pub(crate) number_of_index_blocks: u64,
    pub(crate) key_values_start_point: u64,
    pub(crate) net_block_size_in_bits: u64,
}

impl DbFileHeader {
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
        let items_per_index_block = (block_size as f64 / 4.0).floor() as u64;
        let number_of_index_blocks =
            (max_keys as f64 / items_per_index_block as f64) as u64 + redundant_blocks as u64;
        let last_offset = 800 + (items_per_index_block * 4 * 8 * number_of_index_blocks);
        let key_values_start_point = last_offset + 1;
        let net_block_size_in_bits = items_per_index_block * 4 * 8;

        Self {
            title: b"Scdb versn 0.001",
            block_size,
            max_keys,
            redundant_blocks,
            last_offset,
            items_per_index_block,
            number_of_index_blocks,
            key_values_start_point,
            net_block_size_in_bits,
        }
    }
}
