use crate::internal::entries::headers::inverted_index_header::InvertedIndexHeader;
use crate::internal::entries::headers::shared::{HEADER_SIZE_IN_BYTES, INDEX_ENTRY_SIZE_IN_BYTES};
use crate::internal::entries::index::Index;
use crate::internal::entries::values::inverted_index_entry::InvertedIndexEntry;
use crate::internal::macros::{acquire_lock, validate_bounds};
use crate::internal::utils::get_vm_page_size;
use crate::internal::{slice_to_array, Header, ValueEntry};
use memchr;
use std::cmp::min;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const DEFAULT_MAX_INDEX_KEY_LEN: u32 = 3;
const ZERO_U64_BYTES: [u8; 8] = 0u64.to_be_bytes();

/// The Index for searching for the keys that exist in the database
/// using full text search
#[derive(Debug)]
pub(crate) struct InvertedIndex {
    file: File,
    max_index_key_len: u32,
    values_start_point: u64,
    pub(crate) file_path: PathBuf,
    file_size: u64,
    header: InvertedIndexHeader,
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
            header,
        };

        Ok(v)
    }

    /// Adds a key's kv address in the corresponding prefixes' lists to update the inverted index
    pub(crate) fn add(&mut self, key: &[u8], kv_address: u64, expiry: u64) -> io::Result<()> {
        for i in 1u32..(self.max_index_key_len + 1) {
            let prefix = &key[..i as usize];

            let mut index_block = 0;
            let index_offset = self.header.get_index_offset(prefix);

            loop {
                let index_offset = self
                    .header
                    .get_index_offset_in_nth_block(index_offset, index_block)?;
                let addr = self.read_entry_address(index_offset)?;

                if addr == ZERO_U64_BYTES {
                    self.append_new_root_entry(prefix, index_offset, key, kv_address, expiry)?;
                    break;
                } else if self.addr_belongs_to_prefix(&addr, prefix)? {
                    self.upsert_entry(prefix, &addr, key, kv_address, expiry)?;
                    break;
                }

                index_block += 1;
                if index_block >= self.header.number_of_index_blocks {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "CollisionSaturatedError: no free slot for key: {:?}",
                            prefix
                        ),
                    ));
                }
            }
        }

        Ok(())
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
    pub(crate) fn search(&mut self, term: &[u8], skip: u64, limit: u64) -> io::Result<Vec<u64>> {
        let prefix_len = min(term.len(), self.max_index_key_len as usize);
        let prefix = &term[..prefix_len];

        let mut index_block = 0;
        let index_offset = self.header.get_index_offset(prefix);

        while index_block < self.header.number_of_index_blocks {
            let index_offset = self
                .header
                .get_index_offset_in_nth_block(index_offset, index_block)?;
            let addr = self.read_entry_address(index_offset)?;

            if addr == ZERO_U64_BYTES {
                return Ok(vec![]);
            } else if self.addr_belongs_to_prefix(&addr, prefix)? {
                return self.get_matched_kv_addrs_for_prefix(term, &addr, skip, limit);
            }

            index_block += 1;
        }

        Ok(vec![])
    }

    /// Deletes the key's kv address from all prefixes' lists in the inverted index
    pub(crate) fn remove(&mut self, key: &[u8]) -> io::Result<()> {
        for i in 1u32..(self.max_index_key_len + 1) {
            let prefix = &key[..i as usize];

            let mut index_block = 0;
            let index_offset = self.header.get_index_offset(prefix);

            loop {
                let index_offset = self
                    .header
                    .get_index_offset_in_nth_block(index_offset, index_block)?;
                let addr = self.read_entry_address(index_offset)?;

                if addr == ZERO_U64_BYTES {
                    // prefix does not exist
                    break;
                } else if self.addr_belongs_to_prefix(&addr, prefix)? {
                    self.remove_key_for_prefix(index_offset, &addr, key)?;
                    break;
                }

                index_block += 1;
                if index_block >= self.header.number_of_index_blocks {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Compacts the file, removing expired key offsets to reduce its size
    pub(crate) fn compact(&mut self) -> io::Result<()> {
        let folder = self.file_path.parent().unwrap_or_else(|| Path::new("/"));
        let new_file_path = folder.join("tmp__compact_idx.iscdb");
        let mut new_file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(&new_file_path)?;

        let header = InvertedIndexHeader::from_file(&mut self.file)?;

        // Add headers to new file
        new_file.seek(SeekFrom::Start(0))?;
        new_file.write_all(&header.as_bytes())?;

        let mut file = Mutex::new(&self.file);

        let mut index = Index::new(&file, &header);

        let idx_entry_size = INDEX_ENTRY_SIZE_IN_BYTES as usize;
        let zero = vec![0u8; idx_entry_size];
        let mut idx_offset = HEADER_SIZE_IN_BYTES;
        let mut new_file_offset = header.values_start_point;

        for index_block in &mut index {
            let index_block = index_block?;
            // write index block into new file
            new_file.seek(SeekFrom::Start(idx_offset))?;
            new_file.write_all(&index_block)?;

            let len = index_block.len();
            let mut idx_block_cursor: usize = 0;
            while idx_block_cursor < len {
                let lower = idx_block_cursor;
                let upper = lower + idx_entry_size;
                let idx_bytes = index_block[lower..upper].to_vec();

                if idx_bytes != zero {
                    let root_addr = u64::from_be_bytes(slice_to_array(&idx_bytes[..])?);
                    let mut entry_list =
                        extract_entries_bytes_for_prefix(&file, root_addr, new_file_offset)?;

                    new_file_offset = write_entries_to_file(
                        &mut new_file,
                        idx_offset,
                        new_file_offset,
                        &mut entry_list,
                    )?;
                }

                idx_block_cursor = upper;
                idx_offset += INDEX_ENTRY_SIZE_IN_BYTES;
            }
        }

        self.file = new_file;
        self.file_size = new_file_offset;
        self.header = header;

        fs::remove_file(&self.file_path)?;
        fs::rename(&new_file_path, &self.file_path)?;

        Ok(())
    }

    /// Clears all the data in the search index, except the header, and its original
    /// variables
    pub(crate) fn clear(&mut self) -> io::Result<()> {
        let header = InvertedIndexHeader::new(
            Some(self.header.max_keys),
            Some(self.header.redundant_blocks),
            Some(self.header.block_size),
            Some(self.max_index_key_len),
        );
        self.file_size = header.initialize_file(&mut self.file)?;
        Ok(())
    }

    /// Removes the given key from the cyclic linked list for the given `root_addr`
    fn remove_key_for_prefix(
        &mut self,
        index_addr: u64,
        root_addr: &[u8],
        key: &[u8],
    ) -> io::Result<()> {
        let mut root_addr = u64::from_be_bytes(slice_to_array(&root_addr[..])?);
        let mut addr = root_addr;
        loop {
            let entry_bytes = read_entry_bytes(&mut self.file, addr)?;
            let mut entry = InvertedIndexEntry::from_data_array(&entry_bytes, 0)?;

            if entry.key == key {
                let previous_addr = entry.previous_offset;
                let next_addr = entry.next_offset;

                // Deal with next item
                if next_addr != addr {
                    let next_entry_bytes = read_entry_bytes(&mut self.file, next_addr)?;
                    let mut next_entry = InvertedIndexEntry::from_data_array(&next_entry_bytes, 0)?;

                    next_entry.previous_offset = entry.previous_offset;

                    // if next is the same as previous, treat also as previous
                    if next_addr == previous_addr {
                        next_entry.next_offset = entry.next_offset;
                    }

                    // make next entry a root entry since the one being removed is a root entry
                    if entry.is_root {
                        next_entry.is_root = true;
                        // update the root address so that it does not loop forever
                        root_addr = addr;
                        // update the next_offset of the last entry of the cycle to this addr
                        let last_entry_bytes =
                            read_entry_bytes(&mut self.file, entry.previous_offset)?;
                        let last_entry = InvertedIndexEntry::from_data_array(&last_entry_bytes, 0)?;
                        last_entry.update_next_offset_on_file(
                            &mut self.file,
                            entry.previous_offset,
                            addr,
                        )?;
                    }

                    write_entry_to_file(&mut self.file, next_addr, &next_entry)?;
                };

                // Deal with previous item
                if previous_addr != addr {
                    let prev_entry_bytes = read_entry_bytes(&mut self.file, previous_addr)?;
                    let mut previous_entry =
                        InvertedIndexEntry::from_data_array(&prev_entry_bytes, 0)?;

                    previous_entry.next_offset = entry.next_offset;
                    write_entry_to_file(&mut self.file, previous_addr, &previous_entry)?;
                };

                // Deal with current item
                entry.is_deleted = true;
                write_entry_to_file(&mut self.file, addr, &entry)?;

                // update index:
                // if the entry to delete is at the root, and is the only element, reset the index
                if addr == root_addr && next_addr == addr {
                    self.file.seek(SeekFrom::Start(index_addr))?;
                    self.file.write_all(&ZERO_U64_BYTES)?;
                } else if entry.is_root {
                    // the entry being removed is a root entry but there are other elements after it
                    // Update the index to contain the address of the next entry
                    self.file.seek(SeekFrom::Start(index_addr))?;
                    self.file.write_all(&next_addr.to_be_bytes())?;
                }
            }

            addr = entry.next_offset;
            // we have cycled back to the root entry, so exit
            // The zero check is for data corruption
            if addr == root_addr || addr == 0 {
                break;
            }
        }

        Ok(())
    }

    /// Returns the kv_addresses of all items whose db key contain the given `term`
    fn get_matched_kv_addrs_for_prefix(
        &mut self,
        term: &[u8],
        prefix_root_addr: &Vec<u8>,
        skip: u64,
        limit: u64,
    ) -> io::Result<Vec<u64>> {
        let mut matched_addresses: Vec<u64> = vec![];
        let term_finder = memchr::memmem::Finder::new(term);
        let mut skipped = 0u64;
        let should_slice = limit > 0;

        let root_addr = u64::from_be_bytes(slice_to_array(&prefix_root_addr[..])?);
        let mut addr = root_addr;
        loop {
            let entry_bytes = read_entry_bytes(&mut self.file, addr)?;
            let entry = InvertedIndexEntry::from_data_array(&entry_bytes, 0)?;

            if !entry.is_expired() && term_finder.find(entry.key).is_some() {
                if skipped < skip {
                    skipped += 1;
                } else {
                    matched_addresses.push(entry.kv_address);
                }

                if should_slice && matched_addresses.len() as u64 >= limit {
                    break;
                }
            }

            addr = entry.next_offset;
            // The zero check is for data corruption
            if addr == root_addr || addr == 0 {
                break;
            }
        }
        return Ok(matched_addresses);
    }

    /// Updates an existing entry whose prefix (or index key) is given and key is also as given.
    ///
    /// It starts at the root of the doubly-linked cyclic list for the given prefix,
    /// and looks for the given key. If it finds it, it updates it. If it does not find it, it appends
    /// the new entry to the end of that list.
    fn upsert_entry(
        &mut self,
        prefix: &[u8],
        root_address: &[u8],
        key: &[u8],
        kv_address: u64,
        expiry: u64,
    ) -> io::Result<()> {
        let root_address = u64::from_be_bytes(slice_to_array(&root_address[..])?);
        let mut addr = root_address;

        loop {
            let entry_bytes = read_entry_bytes(&mut self.file, addr)?;
            let mut entry = InvertedIndexEntry::from_data_array(&entry_bytes, 0)?;

            if entry.key == key {
                entry.kv_address = kv_address;
                entry.expiry = expiry;
                write_entry_to_file(&mut self.file, root_address, &entry)?;
                break;
            } else if entry.next_offset == root_address {
                // end of list, append new item to list
                let new_entry = InvertedIndexEntry::new(
                    prefix,
                    key,
                    expiry,
                    false,
                    kv_address,
                    root_address,
                    addr,
                );

                let new_entry_len =
                    write_entry_to_file(&mut self.file, self.file_size, &new_entry)?;

                // update the next offset of the current entry to this address
                entry.update_next_offset_on_file(&mut self.file, addr, self.file_size)?;

                // update the root entry to have its previous offset point to the newly added entry
                let root_entry_bytes = read_entry_bytes(&mut self.file, root_address)?;
                let root_entry = InvertedIndexEntry::from_data_array(&root_entry_bytes, 0)?;
                root_entry.update_previous_offset_on_file(
                    &mut self.file,
                    root_address,
                    self.file_size,
                )?;

                // increment file size by the new entry's size
                self.file_size += new_entry_len as u64;
                break;
            }

            addr = entry.next_offset;
            if addr == root_address || addr == 0 {
                // try to avoid looping forever in case of data corruption or something
                // The zero check is for data corruption
                break;
            }
        }

        Ok(())
    }

    /// Appends a new root entry to the index file, and updates the inverted index's index
    #[inline]
    fn append_new_root_entry(
        &mut self,
        prefix: &[u8],
        index_offset: u64,
        key: &[u8],
        kv_address: u64,
        expiry: u64,
    ) -> io::Result<()> {
        let new_addr = self.file.seek(SeekFrom::End(0))?;
        let entry =
            InvertedIndexEntry::new(prefix, key, expiry, true, kv_address, new_addr, new_addr);
        let entry_as_bytes = entry.as_bytes();
        self.file.write_all(&entry_as_bytes)?;

        // update index
        self.file.seek(SeekFrom::Start(index_offset))?;
        self.file.write_all(&new_addr.to_be_bytes())?;
        self.file_size = new_addr + entry_as_bytes.len() as u64;
        Ok(())
    }

    /// Reads the index at the given address and returns it
    ///
    /// # Errors
    ///
    /// If the address is less than [HEADER_SIZE_IN_BYTES] or [InvertedIndexHeader.values_start_point],
    /// an InvalidData error is returned
    #[inline(always)]
    fn read_entry_address(&mut self, address: u64) -> io::Result<Vec<u8>> {
        validate_bounds!(
            (address, address + INDEX_ENTRY_SIZE_IN_BYTES),
            (HEADER_SIZE_IN_BYTES, self.values_start_point)
        )?;

        let size = INDEX_ENTRY_SIZE_IN_BYTES as usize;

        let mut buf: Vec<u8> = vec![0; size];
        self.file.seek(SeekFrom::Start(address))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Checks to see if entry address belongs to the given `prefix` (i.e. index key)
    ///
    /// It returns false if the address is out of bounds
    /// or when the index key there is not equal to `prefix`.
    pub(crate) fn addr_belongs_to_prefix(
        &mut self,
        address: &[u8],
        prefix: &[u8],
    ) -> io::Result<bool> {
        let address = u64::from_be_bytes(slice_to_array(address)?);
        if address >= self.file_size {
            return Ok(false);
        }

        let prefix_length = prefix.len();
        let mut index_key_size_buf = [0u8; 4];
        self.file.seek(SeekFrom::Start(address + 4))?;
        self.file.read_exact(&mut index_key_size_buf)?;
        let index_key_size = u32::from_be_bytes(index_key_size_buf);

        if prefix_length as u32 != index_key_size {
            return Ok(false);
        }

        let mut index_key_buf = vec![0u8; prefix_length];
        self.file.seek(SeekFrom::Start(address + 8))?;
        self.file.read_exact(&mut index_key_buf)?;

        Ok(index_key_buf == prefix)
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

/// Reads a byte array for an entry at the given address in a file
pub fn read_entry_bytes(file: &mut File, address: u64) -> io::Result<Vec<u8>> {
    let mut size_buf = [0u8; 4];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut size_buf)?;
    let size = u32::from_be_bytes(size_buf);

    let mut buf = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut buf);

    Ok(buf)
}

/// Reads a byte array for an entry at the given address in a file mutex
pub fn read_entry_bytes_from_file_mutex(file: &Mutex<&File>, address: u64) -> io::Result<Vec<u8>> {
    let mut file = acquire_lock!(file)?;

    let mut size_buf = [0u8; 4];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut size_buf)?;
    let size = u32::from_be_bytes(size_buf);

    let mut buf = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(address))?;
    file.read_exact(&mut buf);

    Ok(buf)
}

/// Extracts a contiguous list of entries' bytes for a given prefix for from `old_prefix_addr`,
/// to be transferred to `new_prefix_root_addr` as a contiguous yet doubly linked list
fn extract_entries_bytes_for_prefix(
    file: &Mutex<&File>,
    prefix_root_addr: u64,
    new_prefix_root_addr: u64,
) -> io::Result<Vec<Vec<u8>>> {
    struct Detail {
        prev: u64,
        next: u64,
        size: u64,
    }
    let mut addr = prefix_root_addr;
    let mut detail_list: Vec<Detail> = vec![];
    let mut data_list: Vec<Vec<u8>> = vec![];

    loop {
        let entry_byte_array = read_entry_bytes_from_file_mutex(file, addr)?;
        let mut entry = InvertedIndexEntry::from_data_array(&entry_byte_array, 0)?;
        let entry_size = entry_byte_array.len() as u64;

        if !entry.is_expired() {
            let detail_list_len = detail_list.len();
            if detail_list_len >= 1 {
                let prev_index = detail_list_len - 1;
                // point to previous' next_offset because it was set to point to self
                entry.previous_offset = detail_list[prev_index].next;
                // point to self for next_offset
                entry.next_offset = detail_list[prev_index].prev + detail_list[prev_index].size;
                // update previous' next_offset to point to current
                detail_list[prev_index].next = entry.next_offset;
            } else {
                // point to self, for both previous and next offsets
                entry.previous_offset = new_prefix_root_addr;
                entry.next_offset = new_prefix_root_addr;
                // since this is the first entry, it must be a root entry
                entry.is_root = true;
            }

            data_list.push(entry.as_bytes());
            detail_list.push(Detail {
                next: entry.next_offset,
                prev: entry.previous_offset,
                size: entry_size,
            });
        } else if entry.next_offset == prefix_root_addr {
            // reached end of the list
            break;
        } else {
            addr = entry.next_offset;
        }
    }

    let detail_list_len = detail_list.len();
    if detail_list_len > 1 {
        let last_index = detail_list_len - 1;
        // close the circle:
        // the first's previous_offset points to the last's offset
        // and the last's next_offset points to the first's offset:
        let last_entry_offset = detail_list[last_index].next;
        let first_entry_offset = new_prefix_root_addr;

        let mut first_entry = InvertedIndexEntry::from_data_array(&data_list[0], 0)?;
        let mut last_entry = InvertedIndexEntry::from_data_array(&data_list[last_index], 0)?;

        first_entry.previous_offset = last_entry_offset;
        last_entry.next_offset = first_entry_offset;

        let first_entry_bytes = first_entry.as_bytes();
        let last_entry_bytes = last_entry.as_bytes();

        data_list[0] = first_entry_bytes;
        data_list[last_index] = last_entry_bytes;
    }

    Ok(data_list)
}

/// Writes a linked list of entries for a given prefix, into a file.
///
/// The entries are organized such that they physically follow one after the other
///
/// Returns the offset immediately after the list of entries i.e. the exclusive upper bound
fn write_entries_to_file(
    file: &mut File,
    index_addr: u64,
    dest: u64,
    entries: &mut Vec<Vec<u8>>,
) -> io::Result<u64> {
    let mut upper_bound = dest;
    if entries.len() == 0 {
        // delete the index value for this
        file.seek(SeekFrom::Start(index_addr))?;
        file.write_all(&ZERO_U64_BYTES)?;
    } else {
        for entry in entries {
            file.seek(SeekFrom::Start(upper_bound))?;
            file.write_all(entry)?;
            upper_bound += entry.len() as u64;
        }

        // Update new file index
        file.seek(SeekFrom::Start(index_addr))?;
        file.write_all(&dest.to_be_bytes())?;
    }

    Ok(upper_bound)
}

/// Writes a given entry to the file at the given address, returning the number of bytes written
#[inline(always)]
fn write_entry_to_file(
    file: &mut File,
    address: u64,
    entry: &InvertedIndexEntry<'_>,
) -> io::Result<usize> {
    let entry_as_bytes = entry.as_bytes();
    file.seek(SeekFrom::Start(address))?;
    file.write_all(&entry_as_bytes)?;
    Ok(entry_as_bytes.len())
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

        let mut search = create_search_index(file_name, &test_data);

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

        test_search_results(&mut search, &expected_results);

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
            ("bar", 600, now - 3600), // expired
            ("bare", 90, now + 7200),
            ("barricade", 900, 0),
            ("pig", 80, 0),
        ];

        let mut search = create_search_index(file_name, &test_data);

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

        test_search_results(&mut search, &expected_results);

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

        let mut search = create_search_index(file_name, &test_data);

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

        test_search_results(&mut search, &expected_results);

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

        let mut search = create_search_index(file_name, &test_data);

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

        test_search_results(&mut search, &expected_results);

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

        let mut search = create_search_index(file_name, &test_data);

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

        test_search_results(&mut search, &expected_results);

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
        let mut search = InvertedIndex::new(&Path::new(file_name), None, None, None)
            .expect("create a new instance of SearchIndex");
        search.clear().expect("clear the search");
        // add a series of keys and their offsets
        for (key, offset, expiry) in test_data {
            search
                .add(key.as_bytes(), *offset, *expiry)
                .expect(&format!("add key offset {}", key));
        }

        search
    }

    /// tests the search index's search to see if when searched, the expected results
    /// are returned
    fn test_search_results(
        idx: &mut InvertedIndex,
        expected_results: &Vec<((&str, u64, u64), Vec<u64>)>,
    ) {
        for ((term, skip, limit), expected) in expected_results {
            let got = idx
                .search(term.as_bytes(), *skip, *limit)
                .expect(&format!("search {}", term));

            assert_eq!(got, *expected);
        }
    }
}
