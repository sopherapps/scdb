use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use memmap2::{MmapMut, MmapOptions};

use crate::dmap::entries::DbFileHeader;

/// Memory maps the file at the given file path and returns the mapping
///
/// # Errors
/// Returns errors if file fails to open or its length cannot be read or if the
/// memory mapping cannot be done successfully.
/// See:
/// - [std::fs::OpenOptions.open](std::fs::OpenOptions.open)
/// - [std::fs::File.metadata](std::fs::File.metadata)
/// - [memmap2::MmapOptions.map_mut](memmap2::MmapOptions.map_mut)
///
pub(crate) fn generate_mapping(
    file_path: &Path,
    max_keys: Option<u64>,
    redundant_blocks: Option<u16>,
) -> io::Result<MmapMut> {
    let should_create_new = file_path.exists();
    let mut file = OpenOptions::new()
        .append(true)
        .read(true)
        .create(should_create_new)
        .open(file_path)?;

    let file_size = match should_create_new {
        true => {
            let header = DbFileHeader::new(max_keys, redundant_blocks);
            initialize_db_file(&mut file, &header)?;
            header.last_offset
        }
        false => file.metadata()?.len(),
    };

    unsafe { MmapOptions::new().map_mut(&file) }
}

/// Initializes a new database file, giving it the header and the index place holders
fn initialize_db_file(file: &mut File, header: &DbFileHeader) -> io::Result<()> {
    let header_bytes = header.get_header_as_bytes();
    debug_assert_eq!(header_bytes.len(), 100);

    let index_block_bytes = header.create_empty_index_blocks_bytes();
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header_bytes)?;
    file.write_all(&index_block_bytes)?;
    file.seek(SeekFrom::Start(0))?;

    Ok(())
}
