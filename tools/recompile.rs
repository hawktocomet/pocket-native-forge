use recomp3ds::build;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("ERROR: {message}");
    std::process::exit(1);
}

fn find_ndk() -> PathBuf {
    for key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "ANDROID_NDK"] {
        if let Some(value) = env::var_os(key) {
            let path = PathBuf::from(value);
            if path.is_dir() {
                return path;
            }
        }
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| fail("HOME is not set"));

    let default = home.join(
        "Android/Sdk/ndk/27.2.12479018"
    );

    if default.is_dir() {
        return default;
    }

    fail(format!(
        "Android NDK not found. Set ANDROID_NDK_HOME. Checked {}",
        default.display()
    ));
}

fn run(command: &mut Command, description: &str) {
    let status = command.status()
        .unwrap_or_else(|e| fail(format!("{description}: {e}")));

    if !status.success() {
        fail(format!("{description} failed with {status}"));
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 || args.len() > 4 {
        eprintln!(
            "Usage: pocket-recompile <input.3ds> <export-directory> [shared_font.bin]"
        );
        std::process::exit(2);
    }

    let rom = PathBuf::from(&args[1]);
    let export_root = PathBuf::from(&args[2]);
    let font = args.get(3).map(PathBuf::from);

    if !rom.is_file() {
        fail(format!("ROM not found: {}", rom.display()));
    }

    if let Some(path) = &font {
        if !path.is_file() {
            fail(format!("Shared font not found: {}", path.display()));
        }
    }

    let ndk = find_ndk();
    let host_bin = ndk.join(
        "toolchains/llvm/prebuilt/linux-x86_64/bin"
    );

    let compiler = host_bin.join("aarch64-linux-android30-clang");
    let strip = host_bin.join("llvm-strip");

    if !compiler.is_file() || !strip.is_file() {
        fail(format!(
            "Android NDK compiler or strip tool missing in {}",
            host_bin.display()
        ));
    }

    fs::create_dir_all(&export_root)
        .unwrap_or_else(|e| fail(format!("Cannot create export directory: {e}")));

    println!("========== POCKET RECOMPILER ==========");
    println!("ROM:      {}", rom.display());
    println!("Export:   {}", export_root.display());
    println!("NDK:      {}", ndk.display());
    println!("Compiler: {}", compiler.display());
    println!("Fastmem:  enabled");
    println!();

    println!("========== GENERATING 3DS C CODE ==========");

    let generated = build::generate(
        &rom,
        |title| {
            export_root
                .join(format!("{:016X}", title.program_id()))
                .with_extension("generation")
        },
        None,
        &|event| println!("{event:?}"),
    ).unwrap_or_else(|e| fail(format!("Generation failed: {e}")));

    let title_id = generated.title.program_id();
    let title_name = format!("{title_id:016X}");

    println!();
    println!("Title ID: {:016X}", title_id);
    println!("Generated C files: {}", generated.sources.len());

    if generated.sources.is_empty() {
        fail("Generator returned no C sources");
    }

    let package = export_root.join(&title_name);
    let work = export_root.join(format!(".work-{title_name}"));

    if package.exists() {
        fail(format!(
            "Package already exists; refusing to overwrite: {}",
            package.display()
        ));
    }

    if work.exists() {
        fail(format!(
            "Work directory already exists; refusing to overwrite: {}",
            work.display()
        ));
    }

    let objects = work.join("objects");

    fs::create_dir_all(&objects)
        .unwrap_or_else(|e| fail(format!("Cannot create work directory: {e}")));

    println!();
    println!("========== COMPILING ANDROID ARM64 ==========");
    println!("Sources: {}", generated.sources.len());
    println!("Parallel jobs: 4");
    println!("Compiler flag: -DRECOMP_FASTMEM");

    let sources = generated.sources.clone();
    let source_count = sources.len();
    let next = AtomicUsize::new(0);
    let errors = Mutex::new(Vec::<String>::new());

    thread::scope(|scope| {
        for _ in 0..4 {
            let sources = &sources;
            let next = &next;
            let errors = &errors;
            let compiler = &compiler;
            let generated_dir = &generated.dir;
            let objects = &objects;

            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);

                    if index >= sources.len() {
                        break;
                    }

                    let source = &sources[index];
                    let source_path = generated_dir.join(source);
                    let object_path = objects.join(format!("{index:04}.o"));

                    let output = Command::new(compiler)
                        .arg("-c")
                        .arg(&source_path)
                        .arg("-o")
                        .arg(&object_path)
                        .arg(format!("-I{}", generated_dir.display()))
                        .args([
                            "-DRECOMP_FASTMEM",
                            "-O2",
                            "-ffp-contract=off",
                            "-fno-math-errno",
                            "-w",
                            "-fPIC",
                            "-fvisibility=hidden",
                            "-fno-stack-protector",
                        ])
                        .output();

                    match output {
                        Ok(result) if result.status.success() => {
                            println!(
                                "COMPILED [{}/{}]: {}",
                                index + 1,
                                source_count,
                                source
                            );
                        }
                        Ok(result) => {
                            let detail = String::from_utf8_lossy(&result.stderr);
                            errors.lock().unwrap().push(format!(
                                "{}: {}\n{}",
                                source,
                                result.status,
                                detail
                            ));
                        }
                        Err(error) => {
                            errors.lock().unwrap().push(format!(
                                "{}: {}",
                                source,
                                error
                            ));
                        }
                    }
                }
            });
        }
    });

    let errors = errors.into_inner().unwrap();

    if !errors.is_empty() {
        for error in &errors {
            eprintln!("\n{error}");
        }
        fail(format!(
            "{} source file(s) failed to compile. Work retained at {}",
            errors.len(),
            work.display()
        ));
    }

    println!();
    println!("All {source_count} source files compiled.");

    println!();
    println!("========== LINKING ==========");

    let unstripped = work.join(format!("{title_name}.unstripped.so"));
    let library = work.join(format!("{title_name}.so"));

    let mut link = Command::new(&compiler);
    link.arg("-shared")
        .arg("-nostdlib")
        .arg("-Wl,-z,max-page-size=16384")
        .arg("-o")
        .arg(&unstripped);

    for index in 0..source_count {
        link.arg(objects.join(format!("{index:04}.o")));
    }

    run(&mut link, "Android shared-library link");

    println!("Link succeeded.");

    run(
        Command::new(&strip)
            .arg("--strip-unneeded")
            .arg(&unstripped)
            .arg("-o")
            .arg(&library),
        "Library stripping",
    );

    println!("Stripping succeeded.");

    println!();
    println!("========== PACKAGING ==========");

    fs::create_dir_all(&package)
        .unwrap_or_else(|e| fail(format!("Cannot create package: {e}")));

    fs::copy(&rom, package.join("game.3ds"))
        .unwrap_or_else(|e| fail(format!("Cannot copy ROM: {e}")));

    fs::copy(&library, package.join("title.so"))
        .unwrap_or_else(|e| fail(format!("Cannot copy library: {e}")));

    if let Some(font_path) = &font {
        let sysdata = package.join("sysdata");

        fs::create_dir_all(&sysdata)
            .unwrap_or_else(|e| fail(format!("Cannot create sysdata: {e}")));

        fs::copy(
            font_path,
            sysdata.join("shared_font.bin")
        ).unwrap_or_else(|e| fail(format!("Cannot copy shared font: {e}")));
    }

    let library_size = fs::metadata(package.join("title.so"))
        .map(|m| m.len())
        .unwrap_or(0);

    println!("Package: {}", package.display());
    println!("Title ID: {title_name}");
    println!("Library size: {library_size} bytes");
    println!("Shared font: {}", if font.is_some() { "included" } else { "not included" });

    // Retain generated C and object files only if a build fails.
    fs::remove_dir_all(&work)
        .unwrap_or_else(|e| eprintln!("WARNING: Could not remove work directory: {e}"));

    println!();
    println!("========== RECOMPILATION SUCCEEDED ==========");
    println!("Import this folder into Pocket Native:");
    println!("{}", package.display());
    println!();
}
