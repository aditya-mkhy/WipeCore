use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use clap::{Parser, ValueEnum};
use rand::RngCore;

// Windows stuff
use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

use windows::core::PCWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO};

/// ---- Custom IOCTL + structs for VOLUME_DISK_EXTENTS ----
/// We define these ourselves to avoid depending on missing crate constants/structs.

const IOCTL_VOLUME_BASE: u32 = 'V' as u32; // 'V' = 0x56
const METHOD_BUFFERED: u32 = 0;
const FILE_ANY_ACCESS: u32 = 0;

/// This reproduces the CTL_CODE macro from winioctl.h:
const fn ctl_code(device_type: u32, function: u32, method: u32, access: u32) -> u32 {
    (device_type << 16) | (access << 14) | (function << 2) | method
}

const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 =
    ctl_code(IOCTL_VOLUME_BASE, 0, METHOD_BUFFERED, FILE_ANY_ACCESS);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct DiskExtent {
    DiskNumber: u32,
    StartingOffset: i64,
    ExtentLength: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VolumeDiskExtentsLocal {
    NumberOfDiskExtents: u32,
    Extents: [DiskExtent; 1],
}

/// Wipe mode: zeros or random data.
#[derive(Copy, Clone, Debug, ValueEnum)]
enum WipeMode {
    Zeros,
    Random,
}


#[derive(Parser, Debug)]
#[command(
    name = "WipeCore",
    version,
    about = "File wiper + disk inspection (learning stage before raw disk wipes)"
)]
struct Args {
    /// Target file to wipe (ignored if --disk or --list-disks is used)
    target: Option<String>,

    /// Wipe mode: zeros | random (file mode only)
    #[arg(long, value_enum, default_value_t = WipeMode::Zeros)]
    mode: WipeMode,

    /// Number of overwrite passes (file mode only)
    #[arg(long, default_value_t = 1)]
    passes: u32,

    /// Physical drive number for disk size test, e.g. 0 => \\.\PhysicalDrive0
    ///
    /// If this is provided, file wiping is skipped, only this disk's size is printed.
    #[arg(long)]
    disk: Option<u32>,

    /// List all detected physical disks and show their sizes
    #[arg(long)]
    list_disks: bool,

    /// System disk index (for safety marking), e.g. 1 => \\.\PhysicalDrive1
    ///
    /// If not provided, we try to auto-detect (volume of C: / system drive).
    #[arg(long)]
    system_disk: Option<u32>,
}

fn main() {
    let args = Args::parse();

    // 1) If --list-disks: enumerate all disks and exit
    if args.list_disks {
        if let Err(e) = list_disks(16, args.system_disk) {
            eprintln!("Error while listing disks: {}", e);
        }
        return;
    }

    // 2) If --disk N: show that disk's size and exit
    if let Some(disk_num) = args.disk {
        if let Err(e) = show_disk_size(disk_num) {
            eprintln!("Error while checking disk {}: {}", disk_num, e);
        }
        return;
    }

    // 3) Otherwise, we are in FILE WIPE MODE
    let target_str = match &args.target {
        Some(t) => t,
        None => {
            eprintln!("No target file specified.");
            eprintln!("Usage (file wipe):   wipecore <target> [--mode ...] [--passes ...]");
            eprintln!("Usage (disk size):   wipecore --disk <N>");
            eprintln!("Usage (list disks):  wipecore --list-disks [--system-disk N]");
            return;
        }
    };

    let path = Path::new(target_str);

    // 1) Get file metadata
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "Error: could not read metadata for '{}': {}",
                path.display(),
                e
            );
            return;
        }
    };

    if !metadata.is_file() {
        eprintln!("'{}' is not a regular file.", path.display());
        return;
    }

    let size_bytes = metadata.len();
    if size_bytes == 0 {
        eprintln!("File is empty (0 bytes), nothing to wipe.");
        return;
    }

    println!("Target file: {}", path.display());
    println!(
        "Size:   {} bytes (~{:.2} MiB)",
        size_bytes,
        size_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "Mode:   {}",
        match args.mode {
            WipeMode::Zeros => "zeros",
            WipeMode::Random => "random",
        }
    );
    println!("Passes: {}", args.passes);

    // 2) Ask for confirmation
    if let Err(e) = confirm_wipe(path) {
        eprintln!("Error reading confirmation: {}", e);
        return;
    }

    // 3) Open file for read + write
    let file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "Error: could not open '{}' for writing: {}",
                path.display(),
                e
            );
            return;
        }
    };

    // 4) Perform the wipe (multi-pass)
    if let Err(e) = wipe_file(file, size_bytes, args.mode, args.passes) {
        eprintln!("Wipe failed: {}", e);
        return;
    }

    println!("[+] Wipe completed ({} pass(es)).", args.passes);
}


/// Ask user for confirmation before wiping.
fn confirm_wipe(path: &Path) -> io::Result<()> {
    use std::io::{stdin, stdout};

    println!();
    println!("This will overwrite the file:");
    println!("  {}", path.display());
    println!("This CANNOT be undone.");
    println!();
    println!("Type 'YES' to continue:");

    print!("> ");
    stdout().flush()?; // make sure the prompt appears

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    if input.trim() != "YES" {
        println!("Aborted by user.");
        std::process::exit(0);
    }

    Ok(())
}

/// Overwrite the entire file according to the selected mode and passes.
fn wipe_file(
    mut file: std::fs::File,
    size: u64,
    mode: WipeMode,
    passes: u32,
) -> io::Result<()> {
    const CHUNK_SIZE: usize = 8 * 1024 * 1024;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut rng = rand::thread_rng();

    for pass in 1..=passes {
        println!();
        println!("=== Starting pass {}/{} ===", pass, passes);

        // Always start each pass from the beginning of the file
        file.seek(SeekFrom::Start(0))?;

        let mut written: u64 = 0;

        while written < size {
            let remaining = size - written;
            let to_write = if remaining < CHUNK_SIZE as u64 {
                remaining as usize
            } else {
                CHUNK_SIZE
            };

            match mode {
                WipeMode::Zeros => {
                    // buffer already full of zeros
                }
                WipeMode::Random => {
                    rng.fill_bytes(&mut buffer[..to_write]);
                }
            }

            file.write_all(&buffer[..to_write])?;
            written += to_write as u64;

            let percent = (written as f64 / size as f64) * 100.0;
            println!(
                "Pass {}/{}: {} / {} bytes ({:.1}%)",
                pass, passes, written, size, percent
            );
        }

        file.flush()?;
        println!("=== Finished pass {}/{} ===", pass, passes);
    }

    Ok(())
}


/// Helper: convert Rust &str to Windows wide string (UTF-16 + NUL)
fn to_pcwstr(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}

/// Show size of \\.\PhysicalDrive<disk_num>
fn show_disk_size(disk_num: u32) -> io::Result<()> {
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
    let mut bytes_returned: u32 = 0;

    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            None,
            0,
            Some(&mut length_info as *mut _ as *mut _),
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            Some(&mut bytes_returned),
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

    let size_u = size_i64 as u64;
    let size_gib = size_u as f64 / (1024.0 * 1024.0 * 1024.0);

    println!(
        "Disk {} size: {} bytes (~{:.2} GiB)",
        disk_num, size_u, size_gib
    );

    Ok(())
}

/// Try to detect which PhysicalDriveN holds the system volume (C: or SYSTEMDRIVE).
fn detect_system_disk() -> Option<u32> {
    // 1) Get something like "C:" from env; default to C: if missing.
    let system_drive = env::var("SYSTEMDRIVE").unwrap_or_else(|_| "C:".to_string());

    // Build volume path: \\.\C:
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
            eprintln!(
                "Auto-detect: CreateFileW({}) failed: {}",
                volume_path, e
            );
            return None;
        }
    };

    let mut info: VolumeDiskExtentsLocal = VolumeDiskExtentsLocal {
        NumberOfDiskExtents: 0,
        Extents: [DiskExtent {
            DiskNumber: 0,
            StartingOffset: 0,
            ExtentLength: 0,
        }],
    };
    let mut bytes_returned: u32 = 0;

    let res = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
            None,
            0,
            Some(&mut info as *mut _ as *mut _),
            std::mem::size_of::<VolumeDiskExtentsLocal>() as u32,
            Some(&mut bytes_returned),
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

    let disk_num = info.Extents[0].DiskNumber;
    Some(disk_num)
}

/// List all disks up to max_index-1 and mark which one is system disk
/// (auto-detected, unless overridden by --system-disk).
fn list_disks(max_index: u32, system_disk_arg: Option<u32>) -> io::Result<()> {
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

    let mut any_found = false;

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
            Err(_) => {
                continue; // disk number doesn't exist
            }
        };

        let mut length_info = GET_LENGTH_INFORMATION { Length: 0 };
        let mut bytes_returned: u32 = 0;

        let res = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_LENGTH_INFO,
                None,
                0,
                Some(&mut length_info as *mut _ as *mut _),
                std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        unsafe {
            if let Err(e) = CloseHandle(handle) {
                eprintln!("Warning: CloseHandle failed for {}: {}", path, e);
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
        let size_u = size_i64 as u64;
        let size_gib = size_u as f64 / (1024.0 * 1024.0 * 1024.0);

        any_found = true;
        let mark = if i == system_disk {
            " (SYSTEM DISK)"
        } else {
            ""
        };

        println!("[{}] {} - {:.2} GiB{}", i, path, size_gib, mark);
    }

    if !any_found {
        println!("No physical disks found in range 0..{}.", max_index - 1);
    }

    Ok(())
}
