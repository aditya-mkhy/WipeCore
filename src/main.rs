use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};

use rand::RngCore; // for filling buffer with random bytes

// How we want to wipe the file:
enum WipeMode {
    Zeros,
    Random,
}


fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: wipecore <file_path>");
        return;
    }

    let path = &args[1];

    // Parse mode
    let mode = if args.len() >= 3 {
        parse_mode(&args[2])
    } else {
        WipeMode::Zeros
    };

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
    println!(
        "Mode: {}",
        match mode {
            WipeMode::Zeros => "zeros",
            WipeMode::Random => "random",
        }
    );


    // Ask for simple confirmation
    if let Err(e) = confirm_wipe(path) {
        eprintln!("Error reading confirmation: {}", e);
        return;
    }

    println!("Wipe confirmed...");

    //Open file for read + write
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

    // Perform the wipe (single pass) with mode option
    if let Err(e) = wipe_file(file, size_bytes, mode) {
        eprintln!("Wipe failed: {}", e);
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

/// Overwrite the entire file according to the selected mode.
fn wipe_file(mut file: std::fs::File, size: u64, mode: WipeMode) -> io::Result<()> {
    // Always start from the beginning of the file
    file.seek(SeekFrom::Start(0))?;

    const CHUNK_SIZE: usize = 8 * 1024 * 1024;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut rng = rand::thread_rng();

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
                // buffer already full of zeros from initialization
                // no need to change anything
            }
            WipeMode::Random => {
                // fill only the part we are going to write
                rng.fill_bytes(&mut buffer[..to_write]);
            }
        }

        file.write_all(&buffer[..to_write])?;
        written += to_write as u64;

        let percent = (written as f64 / size as f64) * 100.0;
        println!("Written: {} / {} bytes ({:.1}%)", written, size, percent);
    }

    file.flush()?;
    Ok(())
}

/// Parse the mode string into enum.
fn parse_mode(s: &str) -> WipeMode {
    match s.to_lowercase().as_str() {
        "random" => WipeMode::Random,
        "zeros" => WipeMode::Zeros,
        _ => {
            println!("Unknown mode '{}', defaulting to 'zeros'.", s);
            WipeMode::Zeros
        }
    }
}
