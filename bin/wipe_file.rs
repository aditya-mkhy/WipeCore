use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: wipecore <file_path>");
        return;
    }

    let path = &args[1];

    // 1) Get file metadata
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error: could not read metadata for '{}': {}", path, e);
            return;
        }
    };

    if !metadata.is_file() {
        eprintln!("'{}' is not a regular file.", path);
        return;
    }

    let size_bytes = metadata.len();
    if size_bytes == 0 {
        eprintln!("File is empty (0 bytes), nothing to wipe.");
        return;
    }

    println!("Target file: {}", path);
    println!("Size: {} bytes (~{:.2} MiB)", size_bytes, size_bytes as f64 / (1024.0 * 1024.0));

    // Ask for simple confirmation
    if let Err(e) = confirm_wipe(path) {
        eprintln!("Error reading confirmation: {}", e);
        return;
    }

    println!("Wipe confirmed...");

    // 3) Open file for read + write
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error=>  could not open '{}' for writing: {}", path, e);
            return;
        }
    };

    // 4) Perform the wipe (single pass, zeros)
    if let Err(e) = wipe_with_zeros(file, size_bytes) {
        eprintln!("Wipe faild: {}", e);
        return;
    }

    println!("Wipe completed");


}

/// Asking for confirmation before wiping.
fn confirm_wipe(path: &str) -> io::Result<()> {
    use std::io::{stdin, stdout};

    println!();
    println!("This will overwrite the file with zeros : ");
    println!("  {}", path);
    println!("This CANNOT be undone...");
    println!();
    println!("Type 'YES' to continue : ");

    print!("> ");
    stdout().flush()?; // make prompt appears

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "yes" {
        println!("Aborted by user.");
        std::process::exit(0);
    }

    Ok(())
}

/// Overwrite the entire file with 0's.
fn wipe_with_zeros(mut file: std::fs::File, size: u64) -> io::Result<()> {
    // from beigning
    file.seek(SeekFrom::Start(0))?;

    // write in 8 MiB chunks
    const CHUNK_SIZE: usize = 8 * 1024 * 1024;
    let buffer = vec![0u8; CHUNK_SIZE];

    let mut written: u64 = 0;

    while written < size {
        let remaining = size - written;
        let to_write = if remaining < CHUNK_SIZE as u64 {
            remaining as usize
        } else {
            CHUNK_SIZE
        };

        file.write_all(&buffer[..to_write])?;
        written += to_write as u64;

        // print simple progress every ~10%
        let percent = (written as f64 / size as f64) * 100.0;
        println!("Written: {} / {} bytes ({:.1}%)", written, size, percent);
    }

    file.flush()?;
    Ok(())
}