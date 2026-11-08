use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: generate-test <input.3ds> <output-directory>");
        std::process::exit(2);
    }

    let rom = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);

    if !rom.is_file() {
        eprintln!("ROM not found: {}", rom.display());
        std::process::exit(2);
    }

    println!("ROM: {}", rom.display());
    println!("Output: {}", output.display());
    println!("Starting 3DS analysis and C generation...");

    let event_count = AtomicUsize::new(0);

    match pocket_recompiler_core::generate(&rom, &output, |_| {
        event_count.fetch_add(1, Ordering::Relaxed);
    }) {
        Ok(result) => {
            println!();
            println!("========== GENERATION SUCCEEDED ==========");
            println!("Title ID:       {:016X}", result.title_id);
            println!("Output folder:  {}", result.output_dir.display());
            println!("Generated C files: {}", result.sources.len());
            println!("Progress events:   {}", event_count.load(Ordering::Relaxed));
        }
        Err(error) => {
            eprintln!();
            eprintln!("========== GENERATION FAILED ==========");
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
