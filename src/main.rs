use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: wipecore <file_path>");
        return;
    }

    // Take the second argument as the file path
    let path = &args[1];

    // read metadata of the file (size, etc.)
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
    let size_mib = size_bytes as f64 / (1024.0 * 1024.0);

    println!("File: {}", path);
    println!("Size: {} bytes (~{:.2} MiB)", size_bytes, size_mib);
}
