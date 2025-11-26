use std::env;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO};

use crate::util::{size_format, to_pcwstr};
use crate::wipe::{wipe_file, WipeMode};

const IOCTL_VOLUME_BASE: u32 = 'V' as u32;
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

/// CTL_CODE(DeviceType, Function, Method, Access)
const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 =
    ctl_code(IOCTL_VOLUME_BASE, 0, METHOD_BUFFERED, FILE_ANY_ACCESS);

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DiskExtent {
    DiskNumber: u32,
    StartingOffset: i64,
    ExtentLength: i64,
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VolumeDiskExtentsLocal {
    NumberOfDiskExtents: u32,
    Extents: [DiskExtent; 1],
}

struct DiskInfo {
    index: u32,
    size_bytes: u64,
    is_system: bool,
}

// public API used by main.rs
pub fn show_disk_size(disk_num: u32) -> io::Result<()> {
    let path = format!(r"\\.\PhysicalDrive{}", disk_num);
    println!("Opening physical drive: {}", path);

    let wide = to_pcwstr(&path);

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("CreateFileW failed: {e}")))?;

    let mut length_info = GET_LENGTH_INFORMATION { Length: 0 };
    let mut br: u32 = 0;

    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            None,
            0,
            Some(&mut length_info as *mut _ as *mut _),
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            Some(&mut br),
            None,
        )
    }
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("DeviceIoControl failed: {e}")))?;

    unsafe {
        if let Err(e) = CloseHandle(handle) {
            eprintln!("Warning: CloseHandle failed: {e}");
        }
    }

    let size_i64 = length_info.Length;
    if size_i64 < 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Negative size returned from IOCTL_DISK_GET_LENGTH_INFO",
        ));
    }
    let size = size_i64 as u64;

    println!("Disk {} size: {}", disk_num, size_format(size));

    Ok(())
}

pub fn list_disks(max_index: u32, system_disk_arg: Option<u32>) -> io::Result<()> {
    let system_disk: u32 = match system_disk_arg {
        Some(n) => {
            println!("Using user-specified system disk: PhysicalDrive{}", n);
            n
        }
        None => match detect_system_disk() {
            Some(n) => {
                println!("Auto-detected system disk: PhysicalDrive{}", n);
                n
            }
            None => {
                println!("Could not auto-detect system disk; defaulting to PhysicalDrive0.");
                println!("You can override with: --system-disk <N>");
                0
            }
        },
    };

    println!();
    println!("Detected physical disks (0..{}):", max_index - 1);

    let mut any = false;

    for i in 0..max_index {
        let path = format!(r"\\.\PhysicalDrive{}", i);
        let wide = to_pcwstr(&path);

        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };

        let handle = match handle {
            Ok(h) => h,
            Err(_) => continue,
        };

        let mut length_info = GET_LENGTH_INFORMATION { Length: 0 };
        let mut br: u32 = 0;

        let res = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                None,
                0,
                Some(&mut length_info as *mut _ as *mut _),
                std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
                Some(&mut br),
                None,
            )
        };

        unsafe {
            if let Err(e) = CloseHandle(handle) {
                eprintln!("Warning: CloseHandle failed for {}: {e}", path);
            }
        }

        if let Err(e) = res {
            eprintln!("  [{}] {} - failed to get size: {}", i, path, e);
            continue;
        }

        let size_i64 = length_info.Length;
        if size_i64 <= 0 {
            continue;
        }

        let size = size_i64 as u64;
        any = true;

        let mark = if i == system_disk { " (SYSTEM DISK)" } else { "" };

        println!("[{}] {} - {}{}", i, path, size_format(size), mark);
    }

    if !any {
        println!("No physical disks found in range 0..{}.", max_index - 1);
    }

    Ok(())
}

/// Full disk wipe flow (select disk, protect system, confirm).
pub fn run_disk_wipe_flow(
    mode: WipeMode,
    passes: u32,
    system_disk_arg: Option<u32>,
) -> io::Result<()> {
    const MAX_INDEX: u32 = 16;

    let system_disk = match system_disk_arg {
        Some(n) => {
            println!("Using user-specified system disk: PhysicalDrive{}", n);
            n
        }
        None => match detect_system_disk() {
            Some(n) => {
                println!("Auto-detected system disk: PhysicalDrive{}", n);
                n
            }
            None => {
                println!("Could not auto-detect system disk; defaulting to PhysicalDrive0.");
                println!("You can override with: --system-disk <N>");
                0
            }
        },
    };

    println!();
    println!("=== Disk Wipe Mode ===");
    println!("System disk       : PhysicalDrive{}", system_disk);
    println!("Wipe mode         : {:?}", mode);
    println!("Passes            : {}", passes);
    println!();

    // collect disks
    let mut disks: Vec<DiskInfo> = Vec::new();

    for i in 0..MAX_INDEX {
        let path = format!(r"\\.\PhysicalDrive{}", i);
        let wide = to_pcwstr(&path);

        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };

        let handle = match handle {
            Ok(h) => h,
            Err(_) => continue,
        };

        let mut length_info = GET_LENGTH_INFORMATION { Length: 0 };
        let mut br: u32 = 0;

        let res = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                None,
                0,
                Some(&mut length_info as *mut _ as *mut _),
                std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
                Some(&mut br),
                None,
            )
        };

        unsafe {
            if let Err(e) = CloseHandle(handle) {
                eprintln!("Warning: CloseHandle failed for {}: {e}", path);
            }
        }

        if let Err(e) = res {
            eprintln!("  [{}] {} - failed to get size: {}", i, path, e);
            continue;
        }

        let size_i64 = length_info.Length;
        if size_i64 <= 0 {
            continue;
        }

        let size = size_i64 as u64;
        disks.push(DiskInfo {
            index: i,
            size_bytes: size,
            is_system: i == system_disk,
        });
    }

    if disks.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "No physical disks found.",
        ));
    }

    println!("Available disks:");
    for d in &disks {
        let mark = if d.is_system {
            " (SYSTEM DISK - PROTECTED)"
        } else {
            ""
        };
        println!(
            "  [{}] \\\\.\\PhysicalDrive{} - {}{}",
            d.index,
            d.index,
            size_format(d.size_bytes),
            mark
        );
    }

    let non_system: Vec<&DiskInfo> = disks.iter().filter(|d| !d.is_system).collect();
    if non_system.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "No non-system disks available to wipe.",
        ));
    }

    println!();
    println!("Enter the disk index you want to WIPE (non-system only), or just press Enter to cancel:");

    use std::io::{stdin, stdout};

    print!("> ");
    stdout().flush()?;

    let mut line = String::new();
    stdin().read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        println!("Aborted by user.");
        return Ok(());
    }

    let idx: u32 = trimmed.parse().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid disk index.")
    })?;

    let selected = match non_system.iter().find(|d| d.index == idx) {
        Some(d) => *d,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selected disk is not a valid non-system disk.",
            ));
        }
    };

    println!();
    println!("You selected: \\\\.\\PhysicalDrive{}", selected.index);
    println!("Size:         {}", size_format(selected.size_bytes));
    println!("Mode:         {:?}", mode);
    println!("Passes:       {}", passes);
    println!();
    println!("THIS WILL IRREVERSIBLY ERASE ALL DATA ON THIS DISK.");
    println!("It will NOT touch the system disk (PhysicalDrive{}).", system_disk);
    println!();
    let phrase = format!("WIPE-DISK-{}", selected.index);
    println!("Type EXACTLY: {}", phrase);
    println!("Anything else will cancel.");

    print!("> ");
    stdout().flush()?;

    let mut input2 = String::new();
    stdin().read_line(&mut input2)?;
    if input2.trim() != phrase {
        println!("Aborted by user (confirmation phrase did not match).");
        return Ok(());
    }

    let dev = format!(r"\\.\PhysicalDrive{}", selected.index);
    println!();
    println!("[*] Opening {} for read/write...", dev);

    let disk_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&dev)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to open {} for write: {}", dev, e),
            )
        })?;

    println!(
        "[*] Starting wipe: {} (mode: {:?}, passes: {})",
        dev, mode, passes
    );

    wipe_file(disk_file, selected.size_bytes, mode, passes)?;

    println!();
    println!("[+] Disk wipe completed for {}.", dev);

    Ok(())
}

fn detect_system_disk() -> Option<u32> {
    let system_drive = env::var("SYSTEMDRIVE").unwrap_or_else(|_| "C:".to_string());
    let volume_path = format!(r"\\.\{}", system_drive);
    println!("Attempting to auto-detect system disk via volume: {}", volume_path);

    let wide = to_pcwstr(&volume_path);

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    let handle = match handle {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Auto-detect: CreateFileW({}) failed: {}", volume_path, e);
            return None;
        }
    };

    let mut info = VolumeDiskExtentsLocal {
        NumberOfDiskExtents: 0,
        Extents: [DiskExtent {
            DiskNumber: 0,
            StartingOffset: 0,
            ExtentLength: 0,
        }],
    };
    let mut br: u32 = 0;

    let res = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            None,
            0,
            Some(&mut info as *mut _ as *mut _),
            std::mem::size_of::<VolumeDiskExtentsLocal>() as u32,
            Some(&mut br),
            None,
        )
    };

    unsafe {
        if let Err(e) = CloseHandle(handle) {
            eprintln!("Auto-detect: CloseHandle({}) failed: {}", volume_path, e);
        }
    }

    if let Err(e) = res {
        eprintln!(
            "Auto-detect: DeviceIoControl(IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS) failed: {}",
            e
        );
        return None;
    }

    if info.NumberOfDiskExtents == 0 {
        eprintln!("Auto-detect: volume reports 0 disk extents.");
        return None;
    }

    Some(info.Extents[0].DiskNumber)
}
