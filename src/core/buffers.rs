use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::Path;

pub(crate) struct BufferPool {
    capacity: usize,
    access_log: HashMap<u64, u64>,
    data: HashMap<u64, Buffer>,
    file: File,
    file_size: u64,
}

pub(crate) struct Buffer {
    data: Vec<u8>,
    is_dirty: bool,
    left_offset: u64,
    right_offset: u64,
}

impl Buffer {
    /// Checks if the given address is in this buffer
    #[inline]
    pub(crate) fn contains(&self, address: u64) -> bool {
        self.left_offset <= address && address <= self.right_offset
    }
}

impl BufferPool {
    /// Creates a new BufferPool with the given `capacity` number of Buffers and
    /// for the file at the given path (creating it if necessary)
    pub(crate) fn new(capacity: usize, file_path: &Path) -> io::Result<Self> {
        // Should start a task on a different thread that runs every once in a while
        // to clean any dirty pages (i.e. flush any changes to disk)
        todo!()
    }
    /// Appends a given data array to the file attached to this buffer pool
    pub(crate) fn append(&self, data: &[u8]) -> io::Result<()> {
        // should look for buffer that contains the address: self.file_size as this is the last offset
        // if it is within the buffers, append the data in memory and return
        // else if should find the access_log for the buffer that has the oldest timestamp and replaces it with
        // the new buffer that contains the file_size address
        todo!()
    }

    /// Inserts a given data array at the given address. Do note that this overwrites
    /// the existing data at that address. If you are looking to update to a value that
    /// could have a different length from the previous one, just mark the previous one
    /// as `deleted` using this `insert` then append the new one to the bottom of the file
    /// and update the index to point to the new value.
    pub(crate) fn insert(&self, address: u64, data: &[u8]) -> io::Result<()> {
        // How exactly can one insert a value in a file at a given offset and shift all
        // the values below it?
        // This will overwrite the data. Thus the only way to update is to mark previous
        // values as deleted and append the new one
        // should look for buffer that contains the address
        // if it is within the buffers, append the data in memory and return
        // else if should find the access_log for the buffer that has the oldest timestamp and replaces it with
        // the new buffer that contains the address
        todo!()
    }

    /// This removes any deleted or expired entries from the file. It must first lock the buffer and the file.
    /// In order to be more efficient, it creates a new file, copying only that data which is not deleted or expired
    pub(crate) fn compact_file(&self) -> io::Result<()> {
        // This compacting should be done at a given interval or at the request of the user
        todo!()
    }

    /// Returns the value at the given address, of the given size in bytes
    pub(crate) fn get(&self, address: u64, size: Option<u64>) -> io::Result<Vec<u8>> {
        todo!()
    }

    // FIXME: How do you ensure consistency especially if we are not flushing data straight to disk (or should we)?
    //      Should we have a WAL (write-ahead-log) that we clear the moment everything is written to disk? (complex)
    //      Should we use the buffers only for reading but writes be synchronously done to disk
    //      (afterall we are just appending to the bottom except when updating, but we have to seek to a given entry to mark it as deleted)
    //      Or should we just have the entire index in memory as a buffer and update that one
    //      immediately flushing it to the system on insert, or have the cursor be close to the index to seek for a shorter time [maybe have two sets of buffers, one for index, other for
    //      the data]
    //      Or should we encode deleted offsets in the index by keeping the magnitude but making them negative
    //      e.g. 6 becomes -6 and then flushing that to disk
    //      Or should we have a different file for the index and when accessing a value, we start from the last
    //      inserted? (no)
    //      Or just set the deleted's offset to zero and when compacting, we can ignore the entries
    //      that have no offset recorded in index. No need to seek to the entry to flag it as deleted. (since we have size data, we can hop from entry to entry)
    //      this last one is promising
}
