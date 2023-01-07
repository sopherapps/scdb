use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_INDEX_KEY_LEN: u32 = 3;

/// The Index for searching for the keys that exist in the database
/// using full text search
#[derive(Debug)]
pub(crate) struct SearchIndex {
    file: File,
    max_index_key_len: u32,
    values_start_point: u64,
    pub(crate) file_path: PathBuf,
    file_size: u64,
}

impl SearchIndex {
    /// Initializes a new Search Index
    ///
    /// The max keys used in the search file are `max_index_key_len` * `db_max_keys`
    /// Since we each db key will be represented in the index a number of `max_index_key_len` times
    /// for example the key `food` must have the following index keys: `f`, `fo`, `foo`, `food`.
    pub(crate) fn new(
        file_path: &Path,
        max_index_key_len: Option<u32>,
        db_max_keys: Option<u64>,
        db_redundant_blocks: Option<u16>,
    ) -> io::Result<Self> {
        todo!()
    }

    /// Adds a key's offset in the corresponding prefixes' lists to update the inverted index
    pub(crate) fn add_key_offset(&self, key: &[u8], offset: &[u8], expiry: u64) -> io::Result<()> {
        todo!()
    }

    /// Deletes the key's offsets from all prefixes' lists in the inverted index
    pub(crate) fn delete_key_offset(&self, key: &[u8]) -> io::Result<()> {
        todo!()
    }

    /// Returns list of db offsets corresponding to the given term
    /// The offsets can then be used to get the list of key-values from the db
    ///
    /// It skips the first `skip` number of results and returns not more than
    /// `limit` number of items. This is to avoid using up more memory than can be handled by the
    /// host machine.
    ///
    /// If `limit` is 0, all items are returned since it would make no sense for someone to search
    /// for zero items.
    pub(crate) fn search(&self, term: &[u8], skip: u64, limit: u64) -> io::Result<Vec<u64>> {
        todo!()
    }

    /// Compacts the file, removing expired key offsets to reduce its size
    pub(crate) fn compact_file(&self) -> io::Result<()> {
        todo!()
    }

    /// Clears all the data in the search index, except the header, and its original
    /// variables
    pub(crate) fn clear(&self) -> io::Result<()> {
        todo!()
    }
}

impl PartialEq for SearchIndex {
    fn eq(&self, other: &Self) -> bool {
        self.values_start_point == other.values_start_point
            && self.max_index_key_len == other.max_index_key_len
            && self.file_path == other.file_path
            && self.file_size == other.file_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::internal::entries::headers::search_file_header::SearchFileHeader;
    use serial_test::serial;

    #[test]
    #[serial]
    fn new_with_non_existing_file() {
        type Config<'a> = (&'a Path, Option<u32>, Option<u64>, Option<u16>);

        let file_name = "testdb.iscdb";

        struct Expected {
            max_index_key_len: u32,
            values_start_point: u64,
            file_path: PathBuf,
            file_size: u64,
        }

        let test_data: Vec<(Config<'_>, Expected)> = vec![
            (
                (&Path::new(file_name), None, None, None),
                Expected {
                    max_index_key_len: DEFAULT_MAX_INDEX_KEY_LEN,
                    values_start_point: SearchFileHeader::new(None, None, None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: SearchFileHeader::new(None, None, None, None).values_start_point,
                },
            ),
            (
                (&Path::new(file_name), Some(10), None, None),
                Expected {
                    max_index_key_len: 10,
                    values_start_point: SearchFileHeader::new(None, None, None, Some(10))
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: SearchFileHeader::new(None, None, None, Some(10)).values_start_point,
                },
            ),
            (
                (&Path::new(file_name), None, Some(360), None),
                Expected {
                    max_index_key_len: DEFAULT_MAX_INDEX_KEY_LEN,
                    values_start_point: SearchFileHeader::new(Some(360), None, None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: SearchFileHeader::new(Some(360), None, None, None)
                        .values_start_point,
                },
            ),
            (
                (&Path::new(file_name), None, None, Some(4)),
                Expected {
                    max_index_key_len: DEFAULT_MAX_INDEX_KEY_LEN,
                    values_start_point: SearchFileHeader::new(None, Some(4), None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: SearchFileHeader::new(None, Some(4), None, None).values_start_point,
                },
            ),
        ];

        // delete the file so that SearchIndex::new() can reinitialize it.
        fs::remove_file(&file_name).ok();

        for ((file_path, max_index_key_len, max_keys, redundant_blocks), expected) in test_data {
            let got = SearchIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                .expect("new search index");

            assert_eq!(&got.max_index_key_len, &expected.max_index_key_len);
            assert_eq!(&got.values_start_point, &expected.values_start_point);
            assert_eq!(&got.file_path, &expected.file_path);
            assert_eq!(&got.file_size, &expected.file_size);

            // delete the file so that BufferPool::new() can reinitialize it for the next iteration
            fs::remove_file(&got.file_path).expect(&format!("delete file {:?}", &got.file_path));
        }
    }

    #[test]
    #[serial]
    fn new_with_existing_file() {
        type Config<'a> = (&'a Path, Option<u32>, Option<u64>, Option<u16>);
        let file_name = "testdb.scdb";
        let test_data: Vec<Config<'_>> = vec![
            (&Path::new(file_name), None, None, None),
            (&Path::new(file_name), Some(7), None, None),
            (&Path::new(file_name), None, Some(3000), None),
            (&Path::new(file_name), None, None, Some(6)),
        ];

        for (file_path, max_index_key_len, max_keys, redundant_blocks) in test_data {
            let first = SearchIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                .expect("new search index");
            let second = SearchIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                .expect("new buffer pool");

            assert_eq!(&first, &second);
            // delete the file so that SearchIndex::new() can reinitialize it for the next iteration
            fs::remove_file(&first.file_path)
                .expect(&format!("delete file {:?}", &first.file_path));
        }
    }
}
