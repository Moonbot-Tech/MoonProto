//! Platform memory-page helpers for retained-history warmup.

use std::sync::OnceLock;

pub(crate) fn system_page_size() -> Option<usize> {
    static PAGE_SIZE: OnceLock<Option<usize>> = OnceLock::new();
    *PAGE_SIZE.get_or_init(detect_system_page_size)
}

#[cfg(windows)]
fn detect_system_page_size() -> Option<usize> {
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

    let mut info = SYSTEM_INFO::default();
    // SAFETY: `info` is a valid writable SYSTEM_INFO for the duration of the call.
    unsafe { GetSystemInfo(&mut info) };
    usize::try_from(info.dwPageSize)
        .ok()
        .filter(|page_size| *page_size != 0)
}

#[cfg(unix)]
fn detect_system_page_size() -> Option<usize> {
    // SAFETY: `_SC_PAGESIZE` is a read-only process-wide system query.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    usize::try_from(page_size)
        .ok()
        .filter(|page_size| *page_size != 0)
}

#[cfg(not(any(windows, unix)))]
fn detect_system_page_size() -> Option<usize> {
    None
}

#[cfg(all(windows, any(test, feature = "diagnostics")))]
pub(crate) fn resident_page_count(
    base_address: usize,
    byte_len: usize,
    page_size: usize,
) -> Option<usize> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::ProcessStatus::{
        K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    if byte_len == 0 {
        return Some(0);
    }
    if page_size == 0 {
        return None;
    }

    let end_address = base_address.checked_add(byte_len)?;
    let mut entries = Vec::with_capacity(byte_len.div_ceil(page_size) + 1);
    let mut address = base_address;
    while address < end_address {
        entries.push(PSAPI_WORKING_SET_EX_INFORMATION {
            VirtualAddress: address as *mut c_void,
            ..Default::default()
        });
        let next_address = address.checked_add(page_size - address % page_size)?;
        if next_address <= address {
            return None;
        }
        address = next_address;
    }

    let query_bytes = u32::try_from(std::mem::size_of_val(entries.as_slice())).ok()?;
    // SAFETY: the current-process pseudo-handle is valid and `entries` is a
    // writable array of the exact structures expected by the API.
    let queried = unsafe {
        K32QueryWorkingSetEx(
            GetCurrentProcess(),
            entries.as_mut_ptr().cast::<c_void>(),
            query_bytes,
        )
    };
    if queried == 0 {
        return None;
    }
    Some(
        entries
            .iter()
            .filter(|entry| unsafe { entry.VirtualAttributes.Flags } & 1 != 0)
            .count(),
    )
}

#[cfg(all(not(windows), any(test, feature = "diagnostics")))]
pub(crate) fn resident_page_count(
    _base_address: usize,
    _byte_len: usize,
    _page_size: usize,
) -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nonzero_system_page_size() {
        assert!(system_page_size().is_some_and(|page_size| page_size != 0));
    }
}
