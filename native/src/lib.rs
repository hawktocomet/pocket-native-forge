//! Core integration for Pocket Recompiler.
//!
//! Reuses upstream 3DS analysis and C-generation implementation.
//! Android-specific compilation and packaging will be implemented separately.

use std::path::{Path, PathBuf};
use recomp3ds::build::{self, Event};

pub struct GenerationResult {
    pub title_id: u64,
    pub output_dir: PathBuf,
    pub sources: Vec<String>,
}

/// Generate C sources and headers for a decrypted 3DS ROM.
pub fn generate(
    rom: &Path,
    output_dir: &Path,
    progress: impl Fn(Event) + Sync,
) -> Result<GenerationResult, String> {
    let generated = build::generate(
        rom,
        |_| output_dir.to_path_buf(),
        None,
        &progress,
    )?;

    Ok(GenerationResult {
        title_id: generated.title.program_id(),
        output_dir: generated.dir,
        sources: generated.sources,
    })
}
