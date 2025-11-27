mod util;
mod wipe;
mod win;
mod cli;

use std::fs::OpenOptions;
use std::path::Path;

use crate::cli::parse_args;
use crate::util::size_format;
use crate::wipe::{confirm_wipe, wipe_file};
use crate::win::{list_disks, run_disk_wipe_flow, show_disk_size};

fn main() {
    let args = parse_args();

    // disk wipe mode
    if args.wipe_disk {
        if let Err(e) = run_disk_wipe_flow(args.mode, args.passes, args.system_disk) {
            eprintln!("Disk wipe failed or aborted: {}", e);
        }
        return;
    }

    // just list disks
    if args.list_disks {
        if let Err(e) = list_disks(16, args.system_disk) {
            eprintln!("Error while listing disks: {}", e);
        }
        return;
    }

    // single disk info
    if let Some(disk_num) = args.disk {
        if let Err(e) = show_disk_size(disk_num) {
            eprintln!("Error while checking disk {}: {}", disk_num, e);
        }
        return;
    }

    // file wipe mode
    let target = match &args.target {
        Some(t) => t,
        None => {
            eprintln!("No target file specified.");
            eprintln!("Usage (file wipe):   wipecore <target> [--mode ..] [--passes ..]");
            eprintln!("Usage (disk size):   wipecore --disk <N>");
            eprintln!("Usage (list disks):  wipecore --list-disks [--system-disk N]");
            eprintln!("Usage (disk wipe):   wipecore --wipe-disk [--system-disk N] [--mode ..] [--passes ..]");
            return;
        }
    };

    let path = Path::new(target);

    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("could not read metadata for '{}': {}", path.display(), e);
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

    println!("Target file : {}", path.display());
    println!("Size :   {}", size_format(size_bytes));
    println!("Mode :   {:?}", args.mode);
    println!("Passes : {}", args.passes);

    if let Err(e) = confirm_wipe(path) {
        eprintln!("Error reading confirmation: {}", e);
        return;
    }

    let f = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("could not open '{}' for writing: {}", path.display(), e);
            return;
        }
    };

    if let Err(e) = wipe_file(f, size_bytes, args.mode, args.passes) {
        eprintln!("Wipe failed: {}", e);
        return;
    }

    println!();
    println!("[+] Wipe completed ({} passes).", args.passes);
}