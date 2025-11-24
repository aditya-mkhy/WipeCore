use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use clap::{Parser, ValueEnum};
use rand::RngCore;

// Windows
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
use windows::Win32::System::Ioctl::{
    GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO,
};



/// Wipe mode: zeros or random data.
#[derive(Copy, Clone, Debug, ValueEnum)]
enum WipeMode {
    Zeros,
    Random,
}

/// Modes:
///  - file wipe (default, when --disk is NOT used)
///  - disk info: use --disk N to only print size
#[derive(Parser, Debug)]
#[command(
    name = "WipeCore",
    version,
    about = "Simple file wiper"
)]
struct Args {
    /// Target file to wipe (ignored if --disk is used)
    target: Option<String>,

    /// Wipe mode: zeros | random (file mode only)
    #[arg(long, value_enum, default_value_t = WipeMode::Zeros)]
    mode: WipeMode,

    #[arg(long, default_value_t = 1)]
    passes: u32,

    /// If this is provided, file wiping is skipped, only disk size is printed.
    #[arg(long)]
    disk: Option<u32>,
}

fn main() {
    let args = Args::parse();

    // If --disk is provided => just print disk size and exit
    if let Some(disk_num) = args.disk {
        if let Err(e) = show_disk_size(disk_num) {
            eprintln!("Error while checking disk {}: {}", disk_num, e);
        }
        return;
    }

    // Otherwise, we are in FILE WIPE MODE
    let target_str = match &args.target {
        Some(t) => t,
        None => {
            eprintln!("No target file specified.");
            eprintln!("Usage: wipecore <target> [--mode ...] [--passes ...]");
            eprintln!("Or:    wipecore --disk <N>   # to show disk size");
            return;
        }
    };

    let path = Path::new(target_str);

    // Get file metadata
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

    // Ask for confirmation
    if let Err(e) = confirm_wipe(path) {
        eprintln!("Error reading confirmation: {}", e);
        return;
    }

    // Open file for read + write
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

    // Perform the wipe (multi-pass)
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

// convert Rust &str to Windows wide string (UTF-16 + NUL)
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

    // CreateFileW returns Result<HANDLE>
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,                  // only read access
            FILE_SHARE_READ | FILE_SHARE_WRITE,   // allow others to also access
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("CreateFileW failed: {e}")))?;

    // Prepare buffer for IOCTL_DISK_GET_LENGTH_INFO
    let mut length_info = GET_LENGTH_INFORMATION { Length: 0 };
    let mut bytes_returned: u32 = 0;

    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            None,                                  // no input 
            0,
            Some(&mut length_info as *mut _ as *mut _),
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            Some(&mut bytes_returned),          
            None,
        )
    }
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("DeviceIoControl failed: {e}")))?;

    // close
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
