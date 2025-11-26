use clap::{Parser};

use crate::wipe::WipeMode;

#[derive(Parser, Debug)]
#[command(
    name = "WipeCore",
    version,
    about = "Simple file / disk wiper for Windows"
)]
pub struct Args {
    /// Target file to wipe (ignored in disk modes)
    pub target: Option<String>,

    /// Wipe mode: zeros | random 
    #[arg(long, value_enum, default_value_t = WipeMode::Zeros)]
    pub mode: WipeMode,

    /// Number of overwrite passes
    #[arg(long, default_value_t = 1)]
    pub passes: u32,

    /// Show info for \\.\PhysicalDriveN
    #[arg(long)]
    pub disk: Option<u32>,

    /// List all detected physical disks
    #[arg(long)]
    pub list_disks: bool,

    /// System disk index (override auto-detection)
    #[arg(long)]
    pub system_disk: Option<u32>,

    /// Interactive disk wipe (non-system disks only)
    #[arg(long)]
    pub wipe_disk: bool,
}

pub fn parse_args() -> Args {
    Args::parse()
}
