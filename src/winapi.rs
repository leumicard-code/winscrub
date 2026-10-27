//! Thin wrappers over the Win32 calls winscrub needs.

use anyhow::{anyhow, Result};
use std::collections::HashSet;
use std::ffi::{c_void, OsStr, OsString};
use std::iter::once;
use std::os::windows::ffi::{OsStrExt, OsStringExt};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, ShellExecuteW, SHQUERYRBINFO,
};

const SHERB_NOCONFIRMATION: u32 = 0x0000_0001;
const SHERB_NOPROGRESSUI: u32 = 0x0000_0002;
const SHERB_NOSOUND: u32 = 0x0000_0004;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
const SW_SHOWNORMAL: i32 = 1;
const E_UNEXPECTED: u32 = 0x8000_FFFF;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(once(0)).collect()
}

/// Is this process running elevated?
pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Lowercased image names of every running process we are allowed to see.
pub fn running_processes() -> HashSet<String> {
    let mut set = HashSet::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return set;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = OsString::from_wide(&entry.szExeFile[..len])
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if !name.is_empty() {
                    set.insert(name);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    set
}

/// (bytes, item count) across every drive, or None if the shell refused.
pub fn recycle_bin_info() -> Option<(u64, u64)> {
    unsafe {
        let mut info: SHQUERYRBINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
        let hr = SHQueryRecycleBinW(std::ptr::null(), &mut info);
        if hr == 0 {
            Some((
                info.i64Size.max(0) as u64,
                info.i64NumItems.max(0) as u64,
            ))
        } else {
            None
        }
    }
}

pub fn empty_recycle_bin() -> Result<()> {
    unsafe {
        let hr = SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        );
        // An already empty bin reports E_UNEXPECTED on some builds.
        if hr == 0 || hr as u32 == E_UNEXPECTED {
            Ok(())
        } else {
            Err(anyhow!("SHEmptyRecycleBin failed (0x{:08X})", hr as u32))
        }
    }
}

/// Re-run the current command line through the UAC prompt.
pub fn relaunch_elevated() -> Result<()> {
    let exe = std::env::current_exe()?;
    let params: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--elevate")
        .map(|a| {
            if a.contains(' ') && !a.starts_with('"') {
                format!("\"{a}\"")
            } else {
                a
            }
        })
        .collect();

    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let args = wide(&params.join(" "));

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(anyhow!("the elevation prompt was declined"))
    }
}

/// Turn on ANSI escape handling. Returns false when stdout is redirected.
pub fn enable_virtual_terminal() -> bool {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
            return true;
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}
