use std::env;
use std::fs;
use std::io::{self, Write};

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
