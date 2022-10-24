use crate::internal::DbFileHeader;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

/// This is the Representation of the collection
/// of all Index Entries, iterable block by block
pub(crate) struct Index<'a> {
    num_of_blocks: u64,
    block_size: u64,
    file: &'a Mutex<&'a File>,
    cursor: u64,
}

impl<'a> Index<'a> {
    /// Creates a new index instance
    pub(crate) fn new(file: &'a Mutex<&'a File>, header: &DbFileHeader) -> Self {
        Self {
            num_of_blocks: header.number_of_index_blocks,
            block_size: header.net_block_size,
            file,
            cursor: 0,
        }
    }
}

impl<'a> Iterator for &mut Index<'a> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.num_of_blocks {
            return None;
        }

        let mut file = match self.file.lock() {
            Ok(v) => v,
            Err(e) => return Some(Err(io::Error::new(io::ErrorKind::Other, e.to_string()))),
        };

        let mut data = vec![0u8; self.block_size as usize];
        if let Err(e) = file.seek(SeekFrom::Start(100 + (self.cursor * self.block_size))) {
            return Some(Err(e));
        }

        self.cursor += 1;

        match file.read(&mut data) {
            Ok(_) => Some(Ok(data)),
            Err(e) => Some(Err(e)),
        }
    }
}
