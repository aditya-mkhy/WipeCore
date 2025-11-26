# WipeCore

WipeCore is a fast and simple disk wiping tool written in Rust.  
It can securely erase files or entire physical drives using safe overwrite patterns like zero-fill, random bytes, or the **SecureFlip** (0 → 1) method.

This project is still under development — features get added as I learn and improve.

---

## Features

- **List physical disks** with size & system disk detection  
- **Safe wipe modes**  
  - `zeros` – fill with 0x00  
  - `random` – cryptographically strong random bytes  
  - `secureflip` – 2-pass wipe (zeros → ones)  
- **Raw disk wipe support** (`\\.\PhysicalDriveX`)  
- **Real-time progress bar**  
  - percentage  
  - speed (MiB/s)  
  - ETA  
- **System drive protection**  
- **File wipe support**  
- Confirmations before dangerous operations

---

## Wipe Modes

### **1. Zeros**
Overwrites the entire target with bytes: `00`.

### **2. Random**
Uses strong RNG to overwrite with unpredictable data.

### **3. SecureFlip** *(recommended for HDDs)*
Two-pass overwrite:
1. Pass 1 → `0x00`
2. Pass 2 → `0xFF`

If user sets passes < 2, WipeCore automatically upgrades it to 2.

---

## Usage

Run from an **Administrator terminal** when wiping disks.

### **List all disks**
```
wipecore --list-disks
```

### **Check a disk size**
```
wipecore --disk 0

```

### **Wipe a physical disk**
```
wipecore --wipe-disk --mode secureflip --passes 2
wipecore --wipe-disk --mode zeros --passes 3
wipecore --wipe-disk --mode random --passes 1

```

---

## Warning

This tool **permanently destroys data**.  
If you run it on the wrong disk, everything on that disk will be erased.

Double-check your disk number before wiping.

---
