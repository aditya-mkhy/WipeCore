use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use clap::{Parser, ValueEnum};
use rand::RngCore; // for filling buffer with random bytes

// How we want to wipe the file:
enum WipeMode {
    Zeros,
    Random,
}

// clap agrs parser
#[derive(Parser, Debug)]
#[command(
    name = "WipeCore",
    version,
    about = "Simple file wiper (learning stage before raw disks)"
)]
struct Args {
    /// Target file to wipe
    target: String,

    /// Wipe mode: zeros | random
    #[arg(long, value_enum, default_value_t = WipeMode::Zeros)]
    mode: WipeMode,

    /// Number of overwrite passes
    #[arg(long, default_value_t = 1)]
    passes: u32,
}


fn main() {
    // Parse CLI arguments into Args struct
    let args = Args::parse();
    //path
    let path = Path::new(&args.target);


    // Get file metadata
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
        "Mode:   {}",
        match args.mode {
            WipeMode::Zeros => "zeros",
            WipeMode::Random => "random",
        }
    );
    println!("Passes: {}", args.passes);

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

    // Perform the wipe (multi-pass)
    if let Err(e) = wipe_file(file, size_bytes, args.mode, args.passes) {
        eprintln!("Wipe failed: {}", e);
        return;
    }

    println!("Wipe completed {} passes.", args.passes);

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

// Overwrite the entire file according to the selected mode and passes.
fn wipe_file(
    mut file: std::fs::File, size: u64, mode: WipeMode, passes: u32) -> io::Result<()> {
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
