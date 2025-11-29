use std::fs::File;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;

use clap::ValueEnum;
use rand::RngCore;

use crate::util::format_eta;

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum WipeMode {
    Zeros,
    Random,
    Secureflip,
}

/// Ask user before wiping a file (not used for disk wipe flow).
pub fn confirm_wipe(path: &Path) -> io::Result<()> {
    use std::io::{stdin, stdout};

    println!();
    println!("This will overwrite the file:");
    println!("  {}", path.display());
    println!("This CANNOT be undone.");
    println!();
    println!("Type 'YES' to continue:");

    print!("> ");
    stdout().flush()?; // make sure the prompt shows

    let mut input = String::new();
    stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() != "YES" {
        println!("Aborted by user.");
        std::process::exit(0);
    }

    Ok(())
}

/// Core wipe logic. Works for both files and physical drives.
pub fn wipe_file(
    mut file: File,
    size: u64,
    mode: WipeMode,
    mut passes: u32,
) -> io::Result<()> {
    use std::io::stdout;

    const CHUNK: usize = 8 * 1024 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut rng = rand::thread_rng();

    // SecureFlip should always be at least 2 passes
    if let WipeMode::Secureflip = mode {
        if passes < 2 {
            println!(
                "As you are using 'SecureFlip', passes changed from {} to 2",
                passes
            );
            passes = 2;
        }
    }

    for pass in 1..=passes {
        println!();
        println!("=== Starting pass {}/{} ===", pass, passes);

        file.seek(SeekFrom::Start(0))?;
        let start = Instant::now();
        let mut written: u64 = 0;

        // ---- pre-fill buffer ONCE per pass when pattern is fixed ----
        let static_pattern: Option<u8> = match mode {
            WipeMode::Secureflip => {
                // odd pass -> zeros, even pass -> ones
                if pass % 2 == 1 {
                    Some(0x00)
                } else {
                    Some(0xFF)
                }
            }
            WipeMode::Zeros => Some(0x00),
            WipeMode::Random => None,
        };

        if let Some(byte) = static_pattern {
            buf.fill(byte);
        }
        // --------------------------------------------------------------

        // NEW: throttle progress output
        let mut last_print = Instant::now();

        while written < size {
            let left = size - written;
            let to_write = if left < CHUNK as u64 {
                left as usize
            } else {
                CHUNK
            };

            // For Random mode, we still need fresh random data per chunk
            if matches!(mode, WipeMode::Random) {
                rng.fill_bytes(&mut buf[..to_write]);
            }

            // write the chunk
            file.write_all(&buf[..to_write])?;
            written += to_write as u64;

            // Only update progress every ~200ms or on completion
            if last_print.elapsed().as_millis() >= 200 || written == size {
                let elapsed = start.elapsed();
                let secs = elapsed.as_secs_f64().max(0.000_001);

                let percent = (written as f64 / size as f64) * 100.0;
                let written_mib = written as f64 / (1024.0 * 1024.0);
                let speed_mib_s = written_mib / secs;

                let remain_bytes = size - written;
                let eta_secs = if speed_mib_s > 0.0 {
                    (remain_bytes as f64 / (1024.0 * 1024.0) / speed_mib_s)
                        .max(0.0) as u64
                } else {
                    0
                };
                let eta_str = format_eta(eta_secs);

                print!(
                    "\rPass {}/{}:  {:6.2}%  |  {:7.2} MB/s  |   ETA {}",
                    pass, passes, percent, speed_mib_s, eta_str
                );
                stdout().flush().ok();
                last_print = Instant::now();
            }
        }

        file.flush()?;
        println!();
        println!("=== Finished pass {}/{} ===", pass, passes);
    }

    Ok(())
}
