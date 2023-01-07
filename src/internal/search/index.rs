use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct SearchIndex {
    file: File,
    pub(crate) file_path: PathBuf,
}

impl SearchIndex {
    /// Initializes a new Search Index
    pub(crate) fn new(
        file_path: &Path,
        max_search_index_key_length: Option<usize>,
        max_keys: Option<u64>,
        redundant_blocks: Option<u16>,
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
