use std::fs::OpenOptions;
use std::io;
use std::path::Path;

use memmap2::{MmapMut, MmapOptions};

/// Memory maps the file at the given file path and returns the mapping
///
/// # Errors
/// Returns errors if file fails to open or its length cannot be read or set or if the
/// memory mapping cannot be done successfully.
/// See:
/// - [std::fs::OpenOptions.open](std::fs::OpenOptions.open)
/// - [std::fs::File.metadata](std::fs::File.metadata)
/// - [std::fs::File.set_len](std::fs::File.set_len)
/// - [memmap2::MmapOptions.map_mut](memmap2::MmapOptions.map_mut)
///
pub(crate) fn generate_mapping(file_path: &Path) -> io::Result<MmapMut> {
    let should_create_new = file_path.exists();
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(should_create_new)
        .open(file_path)?;

    let file_size = match should_create_new {
        true => 800,
        false => file.metadata()?.len(),
    };
    file.set_len(file_size)?; // add space for the

    unsafe { MmapOptions::new().map_mut(&file) }
}
