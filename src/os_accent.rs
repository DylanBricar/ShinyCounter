//! OS accent color detection. Falls back to a sensible default per platform.

use crate::types::Color;

/// Returns the system accent color, or a sensible default if detection fails.
pub fn detect() -> Color {
    detect_native().unwrap_or(fallback())
}

pub fn fallback() -> Color {
    // Shiny Counter signature violet.
    Color::new(167, 139, 250)
}

#[cfg(target_os = "windows")]
fn detect_native() -> Option<Color> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    // Read HKCU\Software\Microsoft\Windows\DWM\AccentColor (DWORD).
    // Format: 0xAABBGGRR.
    #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
    type HKEY = *mut std::ffi::c_void;
    const HKEY_CURRENT_USER: HKEY = 0x80000001u32 as usize as HKEY;
    const KEY_READ: u32 = 0x20019;
    const REG_DWORD: u32 = 4;
    const ERROR_SUCCESS: i32 = 0;
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: HKEY,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut HKEY,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: HKEY,
            value_name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegCloseKey(hkey: HKEY) -> i32;
    }

    let sub: Vec<u16> = OsStr::new("Software\\Microsoft\\Windows\\DWM")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let val: Vec<u16> = OsStr::new("AccentColor")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey: HKEY = ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return None;
        }
        let mut data: u32 = 0;
        let mut len: u32 = std::mem::size_of::<u32>() as u32;
        let mut kind: u32 = 0;
        let rc = RegQueryValueExW(
            hkey,
            val.as_ptr(),
            ptr::null_mut(),
            &mut kind,
            &mut data as *mut u32 as *mut u8,
            &mut len,
        );
        RegCloseKey(hkey);
        if rc != ERROR_SUCCESS || kind != REG_DWORD {
            return None;
        }
        let r = (data & 0xFF) as u8;
        let g = ((data >> 8) & 0xFF) as u8;
        let b = ((data >> 16) & 0xFF) as u8;
        Some(Color::new(r, g, b))
    }
}

#[cfg(target_os = "macos")]
fn detect_native() -> Option<Color> {
    // macOS exposes accent color via `defaults read -g AppleAccentColor`.
    // The value maps to a palette index. We translate it to an approximate RGB.
    use std::process::Command;
    let out = Command::new("defaults")
        .args(["read", "-g", "AppleAccentColor"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let idx: i32 = s.parse().ok()?;
    Some(match idx {
        -1 => Color::new(152, 152, 157), // graphite
        0 => Color::new(255, 79, 88),    // red
        1 => Color::new(255, 142, 31),   // orange
        2 => Color::new(255, 197, 30),   // yellow
        3 => Color::new(98, 184, 73),    // green
        4 => Color::new(0, 122, 255),    // blue (default)
        5 => Color::new(149, 84, 168),   // purple
        6 => Color::new(247, 99, 156),   // pink
        _ => Color::new(0, 122, 255),
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn detect_native() -> Option<Color> {
    None
}
