use std::io;
#[cfg(windows)]
use std::mem;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const TRUE_AS_BYTE: u8 = 1;
pub(crate) const FALSE_AS_BYTE: u8 = 0;

#[cfg(windows)]
use winapi::um::sysinfoapi::{GetSystemInfo, LPSYSTEM_INFO, SYSTEM_INFO};

/// Returns the Operating system's virtual memory's page size in bytes
#[cfg(unix)]
#[inline]
pub(crate) fn get_vm_page_size() -> u32 {
    unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) as u32 }
}

/// Returns the Operating system's virtual memory's page size in bytes
#[cfg(windows)]
#[inline]
pub(crate) fn get_vm_page_size() -> u32 {
    unsafe {
        let mut info: SYSTEM_INFO = mem::zeroed();
        GetSystemInfo(&mut info as LPSYSTEM_INFO);

        info.dwPageSize as u32
    }
}

/// Returns the current timestamp in seconds from unix epoch
pub(crate) fn get_current_timestamp() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("System time is poorly configured");
    since_the_epoch.as_secs()
}

/// Creates the database folder if it does not exist
pub(crate) fn initialize_db_folder(store_path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(store_path)
}

/// Extracts a byte array of size N from a byte array slice
pub(crate) fn slice_to_array<const N: usize>(data: &[u8]) -> io::Result<[u8; N]> {
    data.try_into()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Converts a byte array into a boolean
#[inline]
pub(crate) fn byte_array_to_bool(data: &[u8]) -> bool {
    data == [TRUE_AS_BYTE]
}

/// Converts a boolean into a byte array
#[inline]
pub(crate) fn bool_to_byte_array(value: bool) -> &'static [u8; 1] {
    if value {
        &[TRUE_AS_BYTE]
    } else {
        &[FALSE_AS_BYTE]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn get_vm_page_size_returns_page_size() {
        assert!(get_vm_page_size() > 0)
    }

    #[test]
    fn get_current_timestamp_gets_now() {
        let some_time_in_october_2022 = 1666023836u64;
        let now = get_current_timestamp();
        assert!(
            now > some_time_in_october_2022,
            "got = {}, expected = {}",
            now,
            some_time_in_october_2022
        );
    }

    #[test]
    fn initialize_db_folder_creates_non_existing_db_folder() {
        let store_path = Path::new("test_utils_db");
        std::fs::remove_dir_all(store_path).unwrap_or(());
        assert!(!Path::exists(store_path));

        initialize_db_folder(store_path).expect("initializes db folder");

        assert!(Path::exists(store_path));
        std::fs::remove_dir_all(store_path).expect("removes the test_db_utils folder");
    }

    #[test]
    fn slice_to_array_works() {
        let data: Vec<u8> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
        let expected = [3u8, 4u8, 5u8, 6u8];
        let got = slice_to_array::<4>(&data[3..7]).expect("extract 4 bytes starting at index 3");
        assert_eq!(
            &got, &expected,
            "got = {:?}, expected = {:?}",
            &got, &expected
        );
    }

    #[test]
    fn byte_array_to_bool_works() {
        let test_data: [(&[u8], bool); 4] = [
            (&[1][..], true),
            (&[0][..], false),
            (&[1, 1][..], false),
            (&[0, 0, 0, 1, 0, 1][..], false),
        ];

        for (arr, expected) in test_data {
            assert_eq!(byte_array_to_bool(arr), expected);
        }
    }
}
