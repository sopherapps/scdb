#[cfg(windows)]
use std::mem;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use libc;
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
pub(crate) fn initialize_db_folder(store_path: &Path) {
    let _ = std::fs::create_dir_all(store_path);
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::internal::utils::get_vm_page_size;
    use crate::internal::{get_current_timestamp, initialize_db_folder};

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

        initialize_db_folder(store_path);

        assert!(Path::exists(store_path));
        std::fs::remove_dir_all(store_path).expect("removes the test_db_utils folder");
    }
}
