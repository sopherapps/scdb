use crate::internal::{get_hash, utils};
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};

pub(crate) const INDEX_ENTRY_SIZE_IN_BYTES: u64 = 8;
pub(crate) const HEADER_SIZE_IN_BYTES: u64 = 100;

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
            .ceil() as u64
            + self.redundant_blocks as u64;
        self.net_block_size = self.items_per_index_block * INDEX_ENTRY_SIZE_IN_BYTES;
        self.key_values_start_point =
            HEADER_SIZE_IN_BYTES + (self.net_block_size * self.number_of_index_blocks);
    }

    /// Retrieves the byte array that represents the header.
    pub(crate) fn as_bytes(&self) -> Vec<u8> {
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

    /// Extracts the header from the data array
    pub(crate) fn from_data_array(data: &[u8]) -> io::Result<Self> {
        if data.len() < HEADER_SIZE_IN_BYTES as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("data should be at least 100 bytes in length"),
            ));
        }

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
        let data_len = file.read(&mut buf)?;
        if data_len < HEADER_SIZE_IN_BYTES as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("data should be at least 100 bytes in length"),
            ));
        }

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

#[cfg(test)]
mod tests {
    use crate::internal::entries::db_file_header::{
        DbFileHeader, HEADER_SIZE_IN_BYTES, INDEX_ENTRY_SIZE_IN_BYTES,
    };
    use crate::internal::utils::get_vm_page_size;
    use std::fs::{File, OpenOptions};
    use std::io;

    use serial_test::serial;
    use std::io::{Seek, SeekFrom, Write};

    #[test]
    #[serial]
    fn db_file_header_new() {
        let block_size = get_vm_page_size();
        type Record = (Option<u64>, Option<u16>, DbFileHeader);
        let test_table: Vec<Record> = vec![
            (None, None, generate_header(1_000_000, 1, block_size)),
            (
                Some(24_000_000),
                None,
                generate_header(24_000_000, 1, block_size),
            ),
            (None, Some(9), generate_header(1_000_000, 9, block_size)),
            (
                Some(24_000_000),
                Some(5),
                generate_header(24_000_000, 5, block_size),
            ),
        ];

        for (max_keys, redundant_blocks, expected) in test_table {
            let got = DbFileHeader::new(max_keys, redundant_blocks);
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn db_file_header_as_bytes_works() {
        let block_size_bytes = get_vm_page_size().to_be_bytes().to_vec();
        // title: Scdb versn 0.001
        let title_bytes = vec![
            83u8, 99, 100, 98, 32, 118, 101, 114, 115, 110, 32, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 70];
        type Record = (Option<u64>, Option<u16>, Vec<u8>);
        let test_table: Vec<Record> = vec![
            (
                None,
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                Some(24_000_000),
                None,
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                None,
                Some(9),
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
            (
                Some(24_000_000),
                Some(5),
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    reserve_bytes.clone(),
                ]
                .concat(),
            ),
        ];

        for (max_keys, redundant_blocks, expected) in test_table {
            let got = DbFileHeader::new(max_keys, redundant_blocks).as_bytes();
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn db_file_header_from_data_array() {
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: Scdb versn 0.001
        let title_bytes = vec![
            83u8, 99, 100, 98, 32, 118, 101, 114, 115, 110, 32, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 70];
        type Record = (Vec<u8>, DbFileHeader);
        let test_table: Vec<Record> = vec![
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 1, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 1, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 9, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 5, block_size),
            ),
        ];

        for (data_array, expected) in test_table {
            let got = DbFileHeader::from_data_array(&data_array).expect("from_data_array");
            assert_eq!(&got, &expected);
        }
    }

    #[test]
    #[serial]
    fn db_file_header_from_data_array_out_of_bounds() {
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: Scdb versn 0.001
        let title_bytes = vec![
            83u8, 99, 100, 98, 32, 118, 101, 114, 115, 110, 32, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 70];
        let test_table: Vec<Vec<u8>> = vec![
            vec![
                title_bytes[2..].to_vec(), // title is truncated
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 0, 15, 66, 64],
                vec![0, 1],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes[..3].to_vec(), // block_size is truncated
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 1],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 15, 66, 64], // max_keys is truncated
                vec![0, 9],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![5], // redundant_blocks is truncated
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                reserve_bytes[..45].to_vec(), // reserve bytes are truncated
            ]
            .concat(),
        ];

        for data_array in test_table {
            let got = DbFileHeader::from_data_array(&data_array);
            assert!(got.is_err());
        }
    }

    #[test]
    #[serial]
    fn db_file_header_from_file() {
        let file_path = "testdb.scdb";
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: Scdb versn 0.001
        let title_bytes = vec![
            83u8, 99, 100, 98, 32, 118, 101, 114, 115, 110, 32, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 70];
        type Record = (Vec<u8>, DbFileHeader);
        let test_table: Vec<Record> = vec![
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 1, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 1u16 */ vec![0, 1],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 1, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 1_000_000u64 */ vec![0, 0, 0, 0, 0, 15, 66, 64],
                    /* redundant_blocks 9u16 */ vec![0, 9],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(1_000_000, 9, block_size),
            ),
            (
                vec![
                    title_bytes.clone(),
                    block_size_bytes.clone(),
                    /* max_keys 24_000_000u64 */ vec![0, 0, 0, 0, 1, 110, 54, 0],
                    /* redundant_blocks 5u16 */ vec![0, 5],
                    reserve_bytes.clone(),
                ]
                .concat(),
                generate_header(24_000_000, 5, block_size),
            ),
        ];

        for (data_array, expected) in test_table {
            let mut file =
                generate_file_with_data(file_path, &data_array).expect("generate file with data");
            let got = DbFileHeader::from_file(&mut file).expect("from_file");
            assert_eq!(&got, &expected);
        }

        std::fs::remove_file(&file_path).expect("delete the test db file");
    }

    #[test]
    #[serial]
    fn db_file_header_from_data_file_out_of_bounds() {
        let file_path = "testdb.scdb";
        let block_size = get_vm_page_size();
        let block_size_bytes = block_size.to_be_bytes().to_vec();
        // title: Scdb versn 0.001
        let title_bytes = vec![
            83u8, 99, 100, 98, 32, 118, 101, 114, 115, 110, 32, 48, 46, 48, 48, 49,
        ];
        let reserve_bytes = vec![0u8; 70];
        let test_table: Vec<Vec<u8>> = vec![
            vec![
                title_bytes[2..].to_vec(), // title is truncated
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 0, 15, 66, 64],
                vec![0, 1],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes[..3].to_vec(), // block_size is truncated
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 1],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 15, 66, 64], // max_keys is truncated
                vec![0, 9],
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![5], // redundant_blocks is truncated
                reserve_bytes.clone(),
            ]
            .concat(),
            vec![
                title_bytes.clone(),
                block_size_bytes.clone(),
                vec![0, 0, 0, 0, 1, 110, 54, 0],
                vec![0, 5],
                reserve_bytes[..45].to_vec(), // reserve bytes are truncated
            ]
            .concat(),
        ];

        for data_array in test_table {
            let mut file =
                generate_file_with_data(file_path, &data_array).expect("generate file with data");
            let got = DbFileHeader::from_file(&mut file);
            assert!(got.is_err());
        }

        std::fs::remove_file(&file_path).expect("delete the test db file");
    }

    #[test]
    #[serial]
    fn db_file_header_get_index_offset() {
        let db_header = DbFileHeader::new(None, None);
        let offset = db_header.get_index_offset(b"foo");
        let block_1_start = HEADER_SIZE_IN_BYTES;
        let block_1_end = db_header.net_block_size + block_1_start;
        assert!(block_1_start <= offset && offset < block_1_end);
    }

    #[test]
    #[serial]
    fn db_file_header_get_index_offset_in_nth_block() {
        let db_header = DbFileHeader::new(None, None);
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
    fn db_file_header_get_index_offset_in_nth_block_out_of_bounds() {
        let db_header = DbFileHeader::new(None, None);
        let initial_offset = db_header.get_index_offset(b"foo");
        let num_of_blocks = db_header.number_of_index_blocks;

        for i in num_of_blocks..num_of_blocks + 2 {
            assert!(db_header
                .get_index_offset_in_nth_block(initial_offset, i)
                .is_err());
        }
    }

    /// Generates a DbFileHeader basing on the inputs supplied. This is just a helper for tests
    fn generate_header(max_keys: u64, redundant_blocks: u16, block_size: u32) -> DbFileHeader {
        let items_per_index_block =
            (block_size as f64 / INDEX_ENTRY_SIZE_IN_BYTES as f64).floor() as u64;
        let net_block_size = items_per_index_block * INDEX_ENTRY_SIZE_IN_BYTES;
        let number_of_index_blocks = (max_keys as f64 / items_per_index_block as f64).ceil() as u64
            + redundant_blocks as u64;
        let key_values_start_point = 100 + (net_block_size * number_of_index_blocks);

        DbFileHeader {
            title: "Scdb versn 0.001".to_string(),
            block_size,
            max_keys,
            redundant_blocks,
            items_per_index_block,
            number_of_index_blocks,
            key_values_start_point,
            net_block_size,
        }
    }

    /// Generates an empty index array basing on block size, max_keys and redundant_blocks
    fn generate_empty_index_array(
        max_keys: u64,
        redundant_blocks: u16,
        block_size: u32,
    ) -> Vec<u8> {
        let items_per_index_block =
            (block_size as f64 / INDEX_ENTRY_SIZE_IN_BYTES as f64).floor() as u64;
        let number_of_index_blocks = (max_keys as f64 / items_per_index_block as f64).ceil() as u64
            + redundant_blocks as u64;
        vec![
            0;
            (items_per_index_block * number_of_index_blocks * INDEX_ENTRY_SIZE_IN_BYTES) as usize
        ]
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
