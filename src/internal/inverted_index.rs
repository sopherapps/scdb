use crate::internal::entries::headers::inverted_index_header::InvertedIndexHeader;
use crate::internal::utils::get_vm_page_size;
use crate::internal::Header;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

const DEFAULT_MAX_INDEX_KEY_LEN: u32 = 3;

/// The Index for searching for the keys that exist in the database
/// using full text search
#[derive(Debug)]
pub(crate) struct InvertedIndex {
    file: File,
    max_index_key_len: u32,
    values_start_point: u64,
    pub(crate) file_path: PathBuf,
    file_size: u64,
}

impl InvertedIndex {
    /// Initializes a new Inverted Index
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
        let block_size = get_vm_page_size();

        let should_create_new = !file_path.exists();
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(should_create_new)
            .open(file_path)?;

        let header = if should_create_new {
            let header = InvertedIndexHeader::new(
                db_max_keys,
                db_redundant_blocks,
                Some(block_size),
                max_index_key_len,
            );
            header.initialize_file(&mut file)?;
            header
        } else {
            InvertedIndexHeader::from_file(&mut file)?
        };

        let file_size = file.seek(SeekFrom::End(0))?;

        let v = Self {
            file,
            max_index_key_len: header.max_index_key_len,
            values_start_point: header.values_start_point,
            file_path: file_path.into(),
            file_size,
        };

        Ok(v)
    }

    /// Adds a key's kv address in the corresponding prefixes' lists to update the inverted index
    pub(crate) fn add(&self, key: &[u8], offset: &[u8], expiry: u64) -> io::Result<()> {
        todo!()
    }

    /// Deletes the key's kv address from all prefixes' lists in the inverted index
    pub(crate) fn remove(&self, key: &[u8]) -> io::Result<()> {
        todo!()
    }

    /// Returns list of db key-value addresses corresponding to the given term
    /// The addresses can then be used to get the list of key-values from the db
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
    pub(crate) fn compact(&self) -> io::Result<()> {
        todo!()
    }

    /// Clears all the data in the search index, except the header, and its original
    /// variables
    pub(crate) fn clear(&self) -> io::Result<()> {
        todo!()
    }
}

impl PartialEq for InvertedIndex {
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
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom};

    use crate::internal::entries::headers::inverted_index_header::InvertedIndexHeader;
    use crate::internal::get_current_timestamp;
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
                    values_start_point: InvertedIndexHeader::new(None, None, None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: InvertedIndexHeader::new(None, None, None, None).values_start_point,
                },
            ),
            (
                (&Path::new(file_name), Some(10), None, None),
                Expected {
                    max_index_key_len: 10,
                    values_start_point: InvertedIndexHeader::new(None, None, None, Some(10))
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: InvertedIndexHeader::new(None, None, None, Some(10))
                        .values_start_point,
                },
            ),
            (
                (&Path::new(file_name), None, Some(360), None),
                Expected {
                    max_index_key_len: DEFAULT_MAX_INDEX_KEY_LEN,
                    values_start_point: InvertedIndexHeader::new(Some(360), None, None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: InvertedIndexHeader::new(Some(360), None, None, None)
                        .values_start_point,
                },
            ),
            (
                (&Path::new(file_name), None, None, Some(4)),
                Expected {
                    max_index_key_len: DEFAULT_MAX_INDEX_KEY_LEN,
                    values_start_point: InvertedIndexHeader::new(None, Some(4), None, None)
                        .values_start_point,
                    file_path: Path::new(file_name).into(),
                    file_size: InvertedIndexHeader::new(None, Some(4), None, None)
                        .values_start_point,
                },
            ),
        ];

        // delete the file so that SearchIndex::new() can reinitialize it.
        fs::remove_file(&file_name).ok();

        for ((file_path, max_index_key_len, max_keys, redundant_blocks), expected) in test_data {
            let got = InvertedIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                .expect("new search index");

            assert_eq!(&got.max_index_key_len, &expected.max_index_key_len);
            assert_eq!(&got.values_start_point, &expected.values_start_point);
            assert_eq!(&got.file_path, &expected.file_path);
            assert_eq!(&got.file_size, &expected.file_size);

            // delete the file so that SearchIndex::new() can reinitialize it for the next iteration
            fs::remove_file(&got.file_path).expect(&format!("delete file {:?}", &got.file_path));
        }
    }

    #[test]
    #[serial]
    fn new_with_existing_file() {
        type Config<'a> = (&'a Path, Option<u32>, Option<u64>, Option<u16>);
        let file_name = "testdb.iscdb";
        let test_data: Vec<Config<'_>> = vec![
            (&Path::new(file_name), None, None, None),
            (&Path::new(file_name), Some(7), None, None),
            (&Path::new(file_name), None, Some(3000), None),
            (&Path::new(file_name), None, None, Some(6)),
        ];

        for (file_path, max_index_key_len, max_keys, redundant_blocks) in test_data {
            let first =
                InvertedIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                    .expect("new search index");
            let second =
                InvertedIndex::new(file_path, max_index_key_len, max_keys, redundant_blocks)
                    .expect("new buffer pool");

            assert_eq!(&first, &second);
            // delete the file so that SearchIndex::new() can reinitialize it for the next iteration
            fs::remove_file(&first.file_path)
                .expect(&format!("delete file {:?}", &first.file_path));
        }
    }

    #[test]
    #[serial]
    fn add_works() {
        let file_name = "testdb.iscdb";
        let now = get_current_timestamp();

        let test_data = vec![
            ("foo", 20, 0),
            ("food", 60, now + 3600),
            ("fore", 160, 0),
            ("bar", 600, now - 3600), // expired
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let search = create_search_index(file_name, &test_data);

        let expected_results = vec![
            (("f", 0, 0), vec![20, 60, 160]),
            (("fo", 0, 0), vec![20, 60, 160]),
            (("foo", 0, 0), vec![20, 60]),
            (("for", 0, 0), vec![160]),
            (("food", 0, 0), vec![60]),
            (("fore", 0, 0), vec![160]),
            (("b", 0, 0), vec![90, 900]),
            (("ba", 0, 0), vec![90, 900]),
            (("bar", 0, 0), vec![90, 900]),
            (("bare", 0, 0), vec![90]),
            (("barr", 0, 0), vec![900]),
            (("p", 0, 0), vec![80]),
            (("pi", 0, 0), vec![80]),
            (("pig", 0, 0), vec![80]),
        ];

        test_search_results(&search, &expected_results);

        // delete the index file
        fs::remove_file(&search.file_path).expect(&format!("delete file {:?}", &search.file_path));
    }

    #[test]
    #[serial]
    fn search_works() {
        let file_name = "testdb.iscdb";
        let now = get_current_timestamp();
        let test_data = vec![
            ("foo", 20, 0),
            ("food", 60, now + 3600),
            ("fore", 160, 0),
            ("bar", 600, now + 3600),
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let search = create_search_index(file_name, &test_data);

        let expected_results = vec![
            (("f", 0u64, 0u64), vec![20u64, 60, 160]),
            (("f", 1, 0), vec![60, 160]),
            (("f", 2, 0), vec![160]),
            (("f", 3, 0), vec![]),
            (("f", 0, 3), vec![20, 60, 160]),
            (("f", 0, 2), vec![20, 60]),
            (("f", 1, 3), vec![60, 160]),
            (("f", 1, 2), vec![60, 160]),
            (("f", 2, 2), vec![160]),
            (("fo", 0, 0), vec![20, 60, 160]),
            (("fo", 1, 0), vec![60, 160]),
            (("fo", 2, 0), vec![160]),
            (("fo", 1, 1), vec![60]),
            (("bar", 0, 0), vec![90, 900]),
            (("bar", 1, 0), vec![900]),
            (("bar", 1, 1), vec![900]),
            (("bar", 1, 2), vec![900]),
            (("pi", 0, 2), vec![80]),
            (("pi", 1, 2), vec![]),
        ];

        test_search_results(&search, &expected_results);

        // delete the index file
        fs::remove_file(&search.file_path).expect(&format!("delete file {:?}", &search.file_path));
    }

    #[test]
    #[serial]
    fn remove_works() {
        // create the instance of the search index
        let file_name = "testdb.iscdb";
        let now = get_current_timestamp();
        let test_data = vec![
            ("foo", 20, 0),
            ("food", 60, now + 3600),
            ("fore", 160, 0),
            ("bar", 600, now - 3500), // expired
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let search = create_search_index(file_name, &test_data);

        search.remove("foo".as_bytes()).expect("delete foo");
        search.remove("pig".as_bytes()).expect("delete pig");

        let expected_results = vec![
            (("f", 0, 0), vec![60, 160]),
            (("fo", 0, 0), vec![60, 160]),
            (("foo", 0, 0), vec![60]),
            (("for", 0, 0), vec![160]),
            (("food", 0, 0), vec![60]),
            (("fore", 0, 0), vec![160]),
            (("b", 0, 0), vec![90, 900]),
            (("ba", 0, 0), vec![90, 900]),
            (("bar", 0, 0), vec![90, 900]),
            (("bare", 0, 0), vec![90]),
            (("barr", 0, 0), vec![900]),
            (("p", 0, 0), vec![]),
            (("pi", 0, 0), vec![]),
            (("pig", 0, 0), vec![]),
        ];

        test_search_results(&search, &expected_results);

        // delete the index file
        fs::remove_file(&search.file_path).expect(&format!("delete file {:?}", &search.file_path));
    }

    #[test]
    #[serial]
    fn compact_works() {
        let file_name = "testdb.iscdb";

        let now = get_current_timestamp();
        let test_data = vec![
            ("foo", 20, 0),
            ("food", 60, now - 2), // expired
            ("fore", 160, 0),
            ("bar", 600, now - 3600), // expired
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let search = create_search_index(file_name, &test_data);

        // delete one of the keys
        search.remove("pig".as_bytes()).expect("delete pig");

        let original_file_size = get_file_size(&search.file_path);

        search.compact().expect("compact search file");

        let final_file_size = get_file_size(&search.file_path);
        let expected_file_size_reduction = 700u64;

        assert_eq!(
            original_file_size - final_file_size,
            expected_file_size_reduction
        );

        // It still works as expected
        let expected_results = vec![
            (("f", 0, 0), vec![20, 160]),
            (("fo", 0, 0), vec![20, 160]),
            (("foo", 0, 0), vec![60]),
            (("for", 0, 0), vec![160]),
            (("food", 0, 0), vec![60]),
            (("fore", 0, 0), vec![160]),
            (("b", 0, 0), vec![90, 900]),
            (("ba", 0, 0), vec![90, 900]),
            (("bar", 0, 0), vec![90, 900]),
            (("bare", 0, 0), vec![90]),
            (("barr", 0, 0), vec![900]),
            (("p", 0, 0), vec![]),
            (("pi", 0, 0), vec![]),
            (("pig", 0, 0), vec![]),
        ];

        test_search_results(&search, &expected_results);

        // delete the index file
        fs::remove_file(&search.file_path).expect(&format!("delete file {:?}", &search.file_path));
    }

    #[test]
    #[serial]
    fn clear_works() {
        let file_name = "testdb.iscdb";
        let now = get_current_timestamp();
        let test_data = vec![
            ("foo", 20, 0),
            ("food", 60, now + 3600),
            ("fore", 160, 0),
            ("bar", 600, now - 3600), // expired
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let search = create_search_index(file_name, &test_data);

        search.clear().expect("clear");

        let expected_results = vec![
            (("f", 0, 0), vec![]),
            (("fo", 0, 0), vec![]),
            (("foo", 0, 0), vec![]),
            (("for", 0, 0), vec![]),
            (("food", 0, 0), vec![]),
            (("fore", 0, 0), vec![]),
            (("b", 0, 0), vec![]),
            (("ba", 0, 0), vec![]),
            (("bar", 0, 0), vec![]),
            (("bare", 0, 0), vec![]),
            (("barr", 0, 0), vec![]),
            (("p", 0, 0), vec![]),
            (("pi", 0, 0), vec![]),
            (("pig", 0, 0), vec![]),
        ];

        test_search_results(&search, &expected_results);

        // delete the index file
        fs::remove_file(&search.file_path).expect(&format!("delete file {:?}", &search.file_path));
    }

    /// Returns the actual file size of the file at the given path
    fn get_file_size(file_path: &Path) -> u64 {
        let mut file = OpenOptions::new()
            .read(true)
            .open(file_path)
            .expect(&format!("open file {:?}", file_path.as_os_str()));
        file.seek(SeekFrom::End(0)).expect("get file size")
    }

    /// Initializes a new SearchIndex and adds the given test_data
    fn create_search_index(file_name: &str, test_data: &Vec<(&str, u64, u64)>) -> InvertedIndex {
        let search = InvertedIndex::new(&Path::new(file_name), None, None, None)
            .expect("create a new instance of SearchIndex");
        // add a series of keys and their offsets
        for (key, offset, expiry) in test_data {
            search
                .add(key.as_bytes(), &offset.to_be_bytes(), *expiry)
                .expect(&format!("add key offset {}", key));
        }

        search
    }

    /// tests the search index's search to see if when searched, the expected results
    /// are returned
    fn test_search_results(
        search: &InvertedIndex,
        expected_results: &Vec<((&str, u64, u64), Vec<u64>)>,
    ) {
        for ((term, skip, limit), expected) in expected_results {
            let got = search
                .search(term.as_bytes(), *skip, *limit)
                .expect(&format!("search {}", term));

            assert_eq!(got, *expected);
        }
    }
}
