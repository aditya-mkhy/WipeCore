use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

/// format size
pub fn size_format(size_bytes: u64) -> String {
    let mut size = size_bytes as f64;
    const UNITS: [&str; 4] = ["Bytes", "KB", "MB", "GB"]; // limit it into GB
    let mut i = 0;

    while size >= 900.0 && i < UNITS.len() - 1 {
        size /= 1024.0;
        i += 1;
    }

    let unit = UNITS[i];
    let val = if size < 10.0 {
        format!("{:.2}", size)
    } else {
        format!("{:.0}", size)
    };

    format!("{} {}", val, unit)
}

/// Format seconds into "01:23" or "02:10:05"
pub fn format_eta(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

/// Convert &str to a Windows wide string buffer (ending with 0).
pub fn to_pcwstr(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}
