//! Dedupe fingerprints: BLAKE3 of the size plus the first and last 1 MiB.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes hashed from each end of the file.
pub const CHUNK: u64 = 1 << 20;

/// Cheap file identity for dedupe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    /// File size in bytes.
    pub size: u64,
    /// BLAKE3 of `size` (little endian), the first 1 MiB and the last 1 MiB.
    pub hash: [u8; 32],
}

impl Fingerprint {
    /// True when the head and tail cover the whole file, so equal
    /// fingerprints mean equal content and no full hash is needed.
    pub fn covers_whole_file(&self) -> bool {
        self.size <= 2 * CHUNK
    }
}

fn read_range(
    file: &mut File,
    start: u64,
    len: u64,
    hasher: &mut blake3::Hasher,
) -> io::Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = len;
    let mut buf = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buf.len() as u64)).unwrap_or(buf.len());
        let n = file.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(())
}

/// Fingerprint of the file at `path`.
pub fn fingerprint(path: &Path) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());
    read_range(&mut file, 0, size.min(CHUNK), &mut hasher)?;
    if size > CHUNK {
        // The tail never overlaps the head, so a file just over 1 MiB hashes each byte once.
        let tail_start = size.saturating_sub(CHUNK).max(CHUNK);
        read_range(&mut file, tail_start, size - tail_start, &mut hasher)?;
    }
    Ok(Fingerprint {
        size,
        hash: *hasher.finalize().as_bytes(),
    })
}

/// BLAKE3 of the whole file (collision check).
pub fn full_hash(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("opendeck-lib-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir.join(name)
    }

    #[test]
    fn middle_changes_are_invisible_to_the_fingerprint_but_not_to_the_full_hash() {
        let size = 3 * CHUNK as usize;
        let a = vec![7u8; size];
        let mut b = a.clone();
        b[size / 2] = 9;
        let (pa, pb) = (tmp("a.bin"), tmp("b.bin"));
        std::fs::write(&pa, &a).expect("write");
        std::fs::write(&pb, &b).expect("write");
        let (fa, fb) = (fingerprint(&pa).expect("fp"), fingerprint(&pb).expect("fp"));
        assert_eq!(fa, fb);
        assert!(!fa.covers_whole_file());
        assert_ne!(full_hash(&pa).expect("h"), full_hash(&pb).expect("h"));
    }

    #[test]
    fn small_files_are_fully_covered() {
        let p = tmp("small.bin");
        std::fs::write(&p, vec![1u8; 1_500_000]).expect("write");
        let f = fingerprint(&p).expect("fp");
        assert!(f.covers_whole_file());
        let q = tmp("small2.bin");
        let mut data = vec![1u8; 1_500_000];
        data[1_400_000] = 2;
        std::fs::write(&q, data).expect("write");
        assert_ne!(f, fingerprint(&q).expect("fp"), "tail bytes are hashed");
    }
}
