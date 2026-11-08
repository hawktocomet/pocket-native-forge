use recomp3ds::build;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
};

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("ERROR: {message}");
    std::process::exit(1);
}

fn log(file: &Arc<Mutex<fs::File>>, message: impl AsRef<str>) {
    let mut f = file.lock().unwrap();
    let _ = writeln!(f, "{}", message.as_ref());
    let _ = f.flush();
    println!("{}", message.as_ref());
}

fn execute(
    linker: &Path,
    executable: &Path,
    args: &[String],
    work: &Path,
) -> Result<(), String> {
    let tmp = work.join("tmp");
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    let output = Command::new(linker)
        .arg(executable)
        .args(args)
        .env("HOME", work)
        .env("TMPDIR", &tmp)
        .env("PATH", format!(
            "{}:/system/bin",
            executable.parent().unwrap().display()
        ))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not start {}: {e}", executable.display()))?;

    if !output.status.success() {
        return Err(format!(
            "{} failed ({})\n{}",
            executable.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 || args.len() > 5 {
        fail("Usage: device-engine ROM WORK_DIRECTORY TOOLCHAIN_DIRECTORY [SHARED_FONT]");
    }

    let rom = PathBuf::from(&args[1]);
    let work = PathBuf::from(&args[2]);
    let toolchain = PathBuf::from(&args[3]);
    let font = args.get(4).map(PathBuf::from);

    if !rom.is_file() {
        fail(format!("ROM does not exist: {}", rom.display()));
    }

    let clang = toolchain.join("bin/clang");
    let lld = toolchain.join("bin/lld");
    let resource = toolchain.join("lib/clang/22");
    let sysroot = toolchain.join("sysroot");

    for path in [&clang, &lld, &resource, &sysroot] {
        if !path.exists() {
            fail(format!("Missing toolchain component: {}", path.display()));
        }
    }

    let log_file = Arc::new(Mutex::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(work.join("progress.log"))
            .unwrap_or_else(|e| fail(e)),
    ));

    log(&log_file, "Analyzing ROM and generating C sources...");

    let generation = work.join("generation");
    let generated = build::generate(
        &rom,
        |_| generation.clone(),
        None,
        &|event| log(&log_file, format!("{event:?}")),
    ).unwrap_or_else(|e| fail(format!("Generation failed: {e}")));

    let title_id = generated.title.program_id();
    let title_name = format!("{title_id:016X}");

    if generated.sources.is_empty() {
        fail("The generator produced no C source files.");
    }

    log(&log_file, format!(
        "Generated {} C files for title {title_name}.",
        generated.sources.len()
    ));

    let objects = work.join("objects");
    fs::create_dir_all(&objects).unwrap_or_else(|e| fail(e));

    let sources = Arc::new(generated.sources);
    let next = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let total = sources.len();

    log(&log_file, format!("Compiling {total} files on the device (4 workers)..."));

    thread::scope(|scope| {
        for _ in 0..4 {
            let sources = Arc::clone(&sources);
            let next = Arc::clone(&next);
            let errors = Arc::clone(&errors);
            let log_file = Arc::clone(&log_file);
            let generated_dir = generated.dir.clone();
            let objects = objects.clone();
            let clang = clang.clone();
            let sysroot = sysroot.clone();
            let resource = resource.clone();
            let work = work.clone();

            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= sources.len() {
                    break;
                }

                let source = generated_dir.join(&sources[index]);
                let object = objects.join(format!("{index:04}.o"));

                let compiler_args = vec![
                    "--target=aarch64-linux-android28".into(),
                    format!("--sysroot={}", sysroot.display()),
                    format!("-resource-dir={}", resource.display()),
                    format!("-I{}", generated_dir.display()),
                    "-DRECOMP_FASTMEM".into(),
                    "-O2".into(),
                    "-ffp-contract=off".into(),
                    "-fno-math-errno".into(),
                    "-w".into(),
                    "-fPIC".into(),
                    "-fvisibility=hidden".into(),
                    "-fno-stack-protector".into(),
                    "-c".into(),
                    source.display().to_string(),
                    "-o".into(),
                    object.display().to_string(),
                ];

                let result = execute(
                    Path::new("/system/bin/linker64"),
                    &clang,
                    &compiler_args,
                    &work,
                );

                match result {
                    Ok(()) => log(
                        &log_file,
                        format!("Compiled {}/{}: {}", index + 1, total, sources[index]),
                    ),
                    Err(error) => {
                        errors.lock().unwrap().push(error.clone());
                        log(&log_file, format!("FAILED: {}", sources[index]));
                        log(&log_file, error);
                    }
                }
            });
        }
    });

    let errors = errors.lock().unwrap();
    if !errors.is_empty() {
        fail(format!("{} source file(s) failed. Work files were retained.", errors.len()));
    }
    drop(errors);

    log(&log_file, "Linking Android ARM64 shared library...");

    let package = work.join("export").join(&title_name);
    fs::create_dir_all(&package).unwrap_or_else(|e| fail(e));

    let library = package.join("title.so");
    let mut link_args = vec![
        "-flavor".into(),
        "gnu".into(),
        "-shared".into(),
        "-nostdlib".into(),
        "-z".into(),
        "max-page-size=16384".into(),
        "-soname".into(),
        format!("{title_name}.so"),
        "--hash-style=both".into(),
        "-o".into(),
        library.display().to_string(),
    ];

    for index in 0..total {
        link_args.push(objects.join(format!("{index:04}.o")).display().to_string());
    }

    execute(
        Path::new("/system/bin/linker64"),
        &lld,
        &link_args,
        &work,
    ).unwrap_or_else(|e| fail(format!("Link failed: {e}")));

    if !library.is_file() || fs::metadata(&library).map(|m| m.len()).unwrap_or(0) == 0 {
        fail("Linker returned success but did not produce a library.");
    }

    log(&log_file, format!("Library created: {} bytes", library.metadata().unwrap().len()));

    fs::copy(&rom, package.join("game.3ds"))
        .unwrap_or_else(|e| fail(format!("Could not copy ROM: {e}")));

    if let Some(font) = font {
        if font.is_file() {
            let sysdata = package.join("sysdata");
            fs::create_dir_all(&sysdata).unwrap_or_else(|e| fail(e));
            fs::copy(font, sysdata.join("shared_font.bin"))
                .unwrap_or_else(|e| fail("Could not copy shared font"));
        }
    }

    fs::write(work.join("package.path"), package.to_string_lossy().as_bytes())
        .unwrap_or_else(|e| fail(e));

    log(&log_file, format!("PACKAGE READY: {}", package.display()));
    log(&log_file, "RECOMPILATION SUCCEEDED");
}
