#[cfg(unix)]
use libc;
#[cfg(windows)]
use std::mem;
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
