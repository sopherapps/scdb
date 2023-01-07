use crate::internal::entries::headers::shared::{
    DerivedHeaderProps, Header, DEFAULT_DB_MAX_KEYS, HEADER_SIZE_IN_BYTES,
};
use crate::internal::{get_hash, utils};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, PartialEq, Clone, Eq, Ord, PartialOrd)]
pub(crate) struct SearchFileHeader {
    pub(crate) title: String,
    pub(crate) block_size: u32,
    pub(crate) max_keys: u64,
    pub(crate) redundant_blocks: u16,
    pub(crate) items_per_index_block: u64,
    pub(crate) number_of_index_blocks: u64,
    pub(crate) values_start_point: u64,
    pub(crate) net_block_size: u64,
    pub(crate) max_index_key_len: u32,
}

impl SearchFileHeader {
    /// Creates a new SearchFileHeader
    pub(crate) fn new(
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
        block_size: Option<u32>,
        max_index_key_len: Option<u32>,
    ) -> Self {
        let max_index_key_len = max_index_key_len.unwrap_or(3);
        let max_keys = max_keys.unwrap_or(DEFAULT_DB_MAX_KEYS * (max_index_key_len as u64));
        let redundant_blocks = redundant_blocks.unwrap_or(1);
        let block_size = block_size.unwrap_or_else(utils::get_vm_page_size);
        let derived_props = DerivedHeaderProps::new(block_size, max_keys, redundant_blocks);

        let mut header = Self {
            title: "ScdbIndex v0.001".to_string(),
            block_size,
            max_keys,
            redundant_blocks,
            items_per_index_block: derived_props.items_per_index_block,
            number_of_index_blocks: derived_props.number_of_index_blocks,
            values_start_point: derived_props.values_start_point,
            net_block_size: derived_props.net_block_size,
            max_index_key_len,
        };

        header
    }
}

impl Header for SearchFileHeader {
    #[inline(always)]
    fn get_items_per_index_block(&self) -> u64 {
        self.items_per_index_block
    }

    #[inline(always)]
    fn get_number_of_index_blocks(&self) -> u64 {
        self.number_of_index_blocks
    }

    #[inline(always)]
    fn get_net_block_size(&self) -> u64 {
        self.net_block_size
    }

    fn as_bytes(&self) -> Vec<u8> {
        self.title
            .as_bytes()
            .iter()
            .chain(&self.block_size.to_be_bytes())
            .chain(&self.max_keys.to_be_bytes())
            .chain(&self.redundant_blocks.to_be_bytes())
            .chain(&[0u8; 70])
            .map(|v| v.to_owned())
            .collect()
    }

    fn from_data_array(data: &[u8]) -> io::Result<Self> {
        if data.len() < HEADER_SIZE_IN_BYTES as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "data should be at least {} bytes in length",
                    HEADER_SIZE_IN_BYTES
                ),
            ));
        }

        let title = String::from_utf8(data[0..16].to_owned())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let block_size = u32::from_be_bytes(utils::slice_to_array::<4>(&data[16..20])?);
        let max_keys = u64::from_be_bytes(utils::slice_to_array::<8>(&data[20..28])?);
        let redundant_blocks = u16::from_be_bytes(utils::slice_to_array::<2>(&data[28..30])?);
        let max_index_key_len = u32::from_be_bytes(utils::slice_to_array::<4>(&data[30..34])?);
        let derived_props = DerivedHeaderProps::new(block_size, max_keys, redundant_blocks);

        let mut header = Self {
            title,
            block_size,
            max_keys,
            redundant_blocks,
            items_per_index_block: derived_props.items_per_index_block,
            number_of_index_blocks: derived_props.number_of_index_blocks,
            values_start_point: derived_props.values_start_point,
            net_block_size: derived_props.net_block_size,
            max_index_key_len,
        };

        Ok(header)
    }
}

impl Display for SearchFileHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SearchFileHeader {{ title: {}, block_size: {}, max_keys: {}, redundant_blocks: {}, items_per_index_block: {}, number_of_index_blocks: {}, values_start_point: {}, net_block_size: {}, max_index_key_len: {}}}",
               self.title,
               self.block_size,
               self.max_keys,
               self.redundant_blocks,
               self.items_per_index_block,
               self.number_of_index_blocks,
               self.values_start_point,
               self.net_block_size,
               self.max_index_key_len,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::utils::get_vm_page_size;
    use std::fs::{File, OpenOptions};
    use std::io;

    use crate::internal::entries::headers::shared::HEADER_SIZE_IN_BYTES;
    use serial_test::serial;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    #[serial]
    fn search_file_header_new() {
        let block_size = get_vm_page_size();
        type Record = (Option<u64>, Option<u16>, Option<u32>, SearchFileHeader);
        let test_table: Vec<Record> = vec![
            (
                None,
                None,
                None,
                generate_header(3_000_000, 1, block_size, 3),
            ),
            (
                Some(24_000_000),
                None,
                None,
                generate_header(24_000_000, 1, block_size, 3),
            ),
            (
                None,
                Some(9),
                None,
                generate_header(3_000_000, 9, block_size, 3),
            ),
            (
                Some(24_000_000),
                Some(5),
                None,
                generate_header(24_000_000, 5, block_size, 3),
            ),
            (
                None,
                None,
                Some(9),
                generate_header(9_000_000, 5, block_size, 9),
            ),
            (
                Some(1_000_000),
                None,
                Some(9),
                generate_header(1_000_000, 5, block_size, 9),
            ),
            (
                Some(24_000_000),
                Some(6),
                Some(7),
                generate_header(24_000_000, 6, block_size, 7),
            ),
        ];

        for (max_keys, redundant_blocks, max_index_key_len, expected) in test_table {
            let got = SearchFileHeader::new(max_keys, redundant_blocks, None, max_index_key_len);
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn search_file_header_as_bytes_works() {
        let block_size_bytes = get_vm_page_size().to_be_bytes().to_vec();
        // title: "ScdbIndex v0.001"
        let title_bytes = vec![
            83u8, 99, 100, 98, 73, 110, 100, 101, 120, 32, 118, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 66];
        type Record = (Option<u64>, Option<u16>, Option<u32>, Vec<u8>);
        let test_table: Vec<Record> = vec![
            (
                None,
                None,
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 3_000_000u64 */ vec![0, 0, 0, 0, 0, 45, 198, 192],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                Some(24_000_000),
                None,
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                None,
                Some(9),
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 3_000_000u64 */ vec![0, 0, 0, 0, 0, 45, 198, 192],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                None,
                None,
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 9_000_000u64 */ vec![0, 0, 0, 0, 0, 137, 84, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 9u32 */ vec![0, 0, 0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                Some(24_000_000),
                Some(5),
                Some(9),
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    /* max_index_key_len 9u32 */ vec![0, 0, 0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
        ];

        for (max_keys, redundant_blocks, max_index_key_len, expected) in test_table {
            let got = SearchFileHeader::new(max_keys, redundant_blocks, None, max_index_key_len)
                .as_bytes();
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn search_file_header_from_data_array() {
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: "ScdbIndex v0.001"
        let title_bytes = vec![
            83u8, 99, 100, 98, 73, 110, 100, 101, 120, 32, 118, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 66];
        type Record = (Vec<u8>, SearchFileHeader);
        let test_table: Vec<Record> = vec![
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 1, block_size, 3),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 9u32 */ vec![0, 0, 0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 1, block_size, 9),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 9, block_size, 3),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 5, block_size, 3),
            ),
        ];

        for (data_array, expected) in test_table {
            let got = SearchFileHeader::from_data_array(&data_array).expect("from_data_array");
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn search_file_header_from_data_array_out_of_bounds() {
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: "ScdbIndex v0.001"
        let title_bytes = vec![
            83u8, 99, 100, 98, 73, 110, 100, 101, 120, 32, 118, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 66];
        let test_table: Vec<Vec<u8>> = vec![
            vec![
                title_bytes[2..].to_vec(), // title is truncated
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 0, 15, 66, 64],
                vec![0, 1],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes[..3].to_vec(), // block_size is truncated
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 1],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 15, 66, 64], // max_keys is truncated
                vec![0, 9],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![5], // redundant_blocks is truncated
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                vec![0, 0, 3], // max_index_key_len is truncated
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                vec![0, 0, 0, 3],
                reserve_bytes[..45].to_vec(), // reserve bytes are truncated
            ]
            .concat(),
        ];

        for data_array in test_table {
            let got = SearchFileHeader::from_data_array(&data_array);
            assert!(got.is_err());
        }
    }

    #[test]
    #[serial]
    fn search_file_header_from_file() {
        let file_path = "testdb.scdb";
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: "ScdbIndex v0.001"
        let title_bytes = vec![
            83u8, 99, 100, 98, 73, 110, 100, 101, 120, 32, 118, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 66];
        type Record = (Vec<u8>, SearchFileHeader);
        let test_table: Vec<Record> = vec![
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 1, block_size, 3),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 1, block_size, 3),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    /* max_index_key_len 3u32 */ vec![0, 0, 0, 3],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 9, block_size, 3),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    /* max_index_key_len 8u32 */ vec![0, 0, 0, 8],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 5, block_size, 8),
            ),
        ];

        for (data_array, expected) in test_table {
            let mut file =
                generate_file_with_data(file_path, &data_array).expect("generate file with data");
            let got = SearchFileHeader::from_file(&mut file).expect("from_file");
            assert_eq!(&got, &expected);
        }

        std::fs::remove_file(&file_path).expect("delete the test db file");
    }

    #[test]
    #[serial]
    fn search_file_header_from_data_file_out_of_bounds() {
        let file_path = "testdb.scdb";
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: "ScdbIndex v0.001"
        let title_bytes = vec![
            83u8, 99, 100, 98, 73, 110, 100, 101, 120, 32, 118, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 66];
        let test_table: Vec<Vec<u8>> = vec![
            vec![
                title_bytes[2..].to_vec(), // title is truncated
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 0, 15, 66, 64],
                vec![0, 1],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes[..3].to_vec(), // block_size is truncated
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 1],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 15, 66, 64], // max_keys is truncated
                vec![0, 9],
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![5], // redundant_blocks is truncated
                vec![0, 0, 0, 3],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                vec![0, 0, 3], // max_index_key_len is truncated
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                vec![0, 0, 0, 3],
                reserve_bytes[..45].to_vec(), // reserve bytes are truncated
            ]
            .concat(),
        ];

        for data_array in test_table {
            let mut file =
                generate_file_with_data(file_path, &data_array).expect("generate file with data");
            let got = SearchFileHeader::from_file(&mut file);
            assert!(got.is_err());
        }

        std::fs::remove_file(&file_path).expect("delete the test db file");
    }

    #[test]
    #[serial]
    fn search_file_header_get_index_offset() {
        let db_header = SearchFileHeader::new(None, None, None, None);
        let offset = db_header.get_index_offset(b"foo");
        let block_1_start = HEADER_SIZE_IN_BYTES;
        let block_1_end = db_header.net_block_size + block_1_start;
        assert!(block_1_start <= offset && offset < block_1_end);
    }

    #[test]
    #[serial]
    fn search_file_header_get_index_offset_in_nth_block() {
        let db_header = SearchFileHeader::new(None, None, None, None);
        let initial_offset = db_header.get_index_offset(b"foo");
        let num_of_blocks = db_header.number_of_index_blocks;
        for i in 0..num_of_blocks {
            let block_start = HEADER_SIZE_IN_BYTES + (i * db_header.net_block_size);
            let block_end = db_header.net_block_size + block_start;
            let offset = db_header
                .get_index_offset_in_nth_block(initial_offset, i)
                .expect("get_index_offset_in_nth_block");
            assert!(block_start <= offset && offset < block_end);
        }
    }

    #[test]
    #[serial]
    fn search_file_header_get_index_offset_in_nth_block_out_of_bounds() {
        let db_header = SearchFileHeader::new(None, None, None, None);
        let initial_offset = db_header.get_index_offset(b"foo");
        let num_of_blocks = db_header.number_of_index_blocks;

        for i in num_of_blocks..num_of_blocks + 2 {
            assert!(db_header
                .get_index_offset_in_nth_block(initial_offset, i)
                .is_err());
        }
    }

    /// Generates a SearchFileHeader basing on the inputs supplied. This is just a helper for tests
    fn generate_header(
        max_keys: u64,
        redundant_blocks: u16,
        block_size: u32,
        max_index_key_len: u32,
    ) -> SearchFileHeader {
        let derived_props = DerivedHeaderProps::new(block_size, max_keys, redundant_blocks);

        SearchFileHeader {
            title: "ScdbIndex v0.001".to_string(),
            block_size,
            max_keys,
            redundant_blocks,
            items_per_index_block: derived_props.items_per_index_block,
            number_of_index_blocks: derived_props.number_of_index_blocks,
            values_start_point: derived_props.values_start_point,
            net_block_size: derived_props.net_block_size,
            max_index_key_len,
        }
    }

    /// Returns a file that has the given data array written to it.
    fn generate_file_with_data(file_path: &str, data_array: &[u8]) -> io::Result<File> {
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(file_path)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(data_array)?;
        Ok(file)
    }
}
