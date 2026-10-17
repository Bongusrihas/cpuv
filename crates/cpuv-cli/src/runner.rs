use anyhow::{Context, Result, ensure};
use cpuv_relocate::{Environment, compiler_args, is_compiler_query};
use fs2::FileExt;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn state_directory(root: &Path) -> Result<PathBuf> {
    let state = root.join(".cpuv");
    fs::create_dir_all(&state)?;
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
        }
        let attributes = fs::metadata(&state)?.file_attributes();
        if attributes & 2 == 0 {
            let wide: Vec<u16> = state.as_os_str().encode_wide().chain(Some(0)).collect();
            // The nul-terminated buffer remains valid for the synchronous call.
            let result = unsafe { SetFileAttributesW(wide.as_ptr(), attributes | 2) };
            if result == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("Hiding the .cpuv build/environment directory");
            }
        }
    }
    Ok(state)
}

pub(crate) fn compiler_command(name: &OsStr) -> bool {
    Path::new(name)
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|s| {
            let lower = s.to_ascii_lowercase();
            let driver = lower.strip_suffix(".exe").unwrap_or(&lower);
            matches!(driver, "g++" | "c++") || driver.ends_with("-g++") || driver.ends_with("-c++")
        })
}

fn source(path: &OsStr) -> bool {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|s| matches!(s, "cpp" | "cc" | "cxx" | "c++" | "C" | "CPP" | "c"))
}

fn link_input(path: &OsStr) -> bool {
    Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "o" | "obj" | "a" | "lib"))
}

fn artifact(build: &Path, requested: &OsStr) -> Result<PathBuf> {
    let name = Path::new(requested)
        .file_name()
        .context("Output must name a file")?;
    let relative = cpuv_store::safe_relative(Path::new(name))?;
    ensure!(
        relative.components().count() == 1,
        "Output must name a file"
    );
    Ok(build.join(relative))
}

fn takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "-I" | "-isystem"
            | "-iquote"
            | "-include"
            | "-imacros"
            | "-L"
            | "-l"
            | "-x"
            | "-B"
            | "-idirafter"
            | "-isysroot"
            | "--sysroot"
            | "-Xlinker"
            | "-Xassembler"
            | "-Xpreprocessor"
            | "-MT"
            | "-MQ"
    )
}

#[derive(Debug)]
struct Plan {
    commands: Vec<Vec<OsString>>,
    executable: Option<PathBuf>,
    program_args: Vec<OsString>,
}

fn plan(root: &Path, build: &Path, input: &[OsString]) -> Result<Plan> {
    let divider = input.iter().position(|s| s == "--");
    let (input, program_args) =
        divider.map_or((input, vec![]), |i| (&input[..i], input[i + 1..].to_vec()));
    ensure!(
        !input.iter().any(|s| s.to_string_lossy().starts_with('@')),
        "Phase A does not yet support compiler response files; pass arguments directly"
    );
    let query = is_compiler_query(input);
    if query {
        return Ok(Plan {
            commands: vec![input.to_vec()],
            executable: None,
            program_args,
        });
    }
    let mut args = Vec::new();
    let mut sources = Vec::new();
    let mut objects = Vec::new();
    let mut output = None;
    let mut i = 0;
    while i < input.len() {
        let arg = &input[i];
        let text = arg.to_string_lossy();
        if arg == "-o" {
            i += 1;
            output = Some(
                input
                    .get(i)
                    .context("-o requires an output filename")?
                    .clone(),
            );
        } else if text.starts_with("-o") && text.len() > 2 {
            output = Some(OsString::from(&text[2..]));
        } else if matches!(
            text.as_ref(),
            "-MF" | "-MJ" | "-dumpdir" | "-dumpbase" | "-dumpbase-ext"
        ) {
            i += 1;
            let value = input
                .get(i)
                .with_context(|| format!("{text} requires a value"))?;
            // The managed runner owns dump paths. Explicit dependency outputs
            // retain their names but are relocated under the hidden build root.
            if text == "-MF" || text == "-MJ" {
                args.push(arg.clone());
                args.push(artifact(build, value)?.into_os_string());
            }
        } else if text.starts_with("-MF") && text.len() > 3 {
            args.push("-MF".into());
            args.push(artifact(build, OsStr::new(&text[3..]))?.into_os_string());
        } else if takes_value(&text) {
            args.push(arg.clone());
            i += 1;
            args.push(
                input
                    .get(i)
                    .with_context(|| format!("{text} requires a value"))?
                    .clone(),
            );
        } else if !text.starts_with('-') && source(arg) {
            sources.push(args.len());
            args.push(arg.clone());
        } else if !text.starts_with('-') && link_input(arg) {
            let existing = root.join(arg);
            let hidden = artifact(build, arg)?;
            objects.push(args.len());
            args.push(if !existing.is_file() && hidden.is_file() {
                hidden.into_os_string()
            } else {
                arg.clone()
            });
        } else if let Some(options) = text.strip_prefix("-Wl,") {
            let mut options: Vec<String> = options.split(',').map(str::to_owned).collect();
            let mut n = 0;
            while n < options.len() {
                for flag in [
                    "-Map",
                    "--Map",
                    "--out-implib",
                    "--output-def",
                    "--dependency-file",
                ] {
                    if options[n] == flag {
                        n += 1;
                        let value = options
                            .get(n)
                            .context("Linker output option requires a filename")?;
                        options[n] = cpuv_relocate::portable(&artifact(build, OsStr::new(value))?);
                        break;
                    } else if let Some(value) = options[n].strip_prefix(&format!("{flag}=")) {
                        options[n] = format!(
                            "{flag}={}",
                            cpuv_relocate::portable(&artifact(build, OsStr::new(value))?)
                        );
                        break;
                    }
                }
                n += 1;
            }
            args.push(format!("-Wl,{}", options.join(",")).into());
        } else {
            args.push(arg.clone());
        }
        i += 1;
    }
    let has = |flag: &str| args.iter().any(|s| s == flag);
    let objects_only = has("-c");
    let assembly = has("-S");
    let no_file_default = has("-E") || has("-M") || has("-MM") || has("-fsyntax-only");
    let library = has("-shared");
    let relocatable = has("-r");
    let produces_executable =
        !objects_only && !assembly && !no_file_default && !library && !relocatable;
    let suffix = if objects_only || relocatable {
        "o"
    } else if assembly {
        "s"
    } else if library {
        "dll"
    } else {
        "exe"
    };
    let stem = sources
        .first()
        .or(objects.first())
        .and_then(|n| Path::new(&args[*n]).file_stem())
        .unwrap_or(OsStr::new("a"));
    let mut default_name = stem.to_owned();
    default_name.push(format!(".{suffix}"));
    let mut result = match output.as_ref() {
        Some(name) => Some(artifact(build, name)?),
        None if !no_file_default => Some(artifact(build, &default_name)?),
        None => None,
    };
    if produces_executable
        && let Some(path) = &mut result
        && !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
    {
        let mut name = path.as_os_str().to_owned();
        name.push(".exe");
        *path = name.into();
    }
    // All auxiliary files and compiler temporaries stay under the build root.
    args.push("-dumpdir".into());
    let mut dumpdir = build.as_os_str().to_owned();
    dumpdir.push(std::path::MAIN_SEPARATOR.to_string());
    args.push(dumpdir);
    let mut commands = Vec::new();
    if (objects_only || assembly) && sources.len() > 1 && output.is_none() {
        let mut seen = std::collections::BTreeSet::new();
        for position in &sources {
            let name = Path::new(&args[*position]).with_extension(suffix);
            let to = artifact(build, name.as_os_str())?;
            ensure!(
                seen.insert(to.clone()),
                "Sources share an output basename; compile them separately with distinct -o names"
            );
            let mut command: Vec<_> = args
                .iter()
                .enumerate()
                .filter(|(n, _)| !sources.contains(n) || n == position)
                .map(|(_, s)| s.clone())
                .collect();
            command.push("-o".into());
            command.push(to.into_os_string());
            commands.push(command);
        }
    } else {
        if let Some(path) = &result {
            args.push("-o".into());
            args.push(path.as_os_str().to_owned());
        }
        commands.push(args);
    }
    Ok(Plan {
        commands,
        executable: if produces_executable { result } else { None },
        program_args,
    })
}

fn runtime_path(root: &Path, env: &Path, metadata: &Environment) -> Result<OsString> {
    let mut paths = vec![
        root.join(".cpuv/bin"),
        env.join("bin"),
        metadata.compiler_bin.clone(),
    ];
    if let Some(host) = cpuv_relocate::msys_host_bin(&metadata.compiler_bin) {
        paths.push(host);
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    Ok(std::env::join_paths(paths)?)
}

fn status(command: &mut Command) -> Result<i32> {
    Ok(command
        .status()
        .context("Running project command")?
        .code()
        .unwrap_or(1))
}

fn missing_header_help(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    let header = text.lines().find_map(|line| {
        line.split_once("fatal error: ")?
            .1
            .split_once(": No such file or directory")
            .map(|(name, _)| name)
    })?;
    Some(format!(
        "cpuv: missing header `{header}`. Add the package that provides it with `cpuv add <package>`; for your own headers, check the filename and pass the containing directory with `-I`. No unverified package was guessed or installed."
    ))
}

fn compiler_status(command: &mut Command) -> Result<(i32, Vec<u8>)> {
    let mut child = command
        .stderr(Stdio::piped())
        .spawn()
        .context("Launching the C++ compiler")?;
    let mut stderr = child.stderr.take().context("Compiler stderr unavailable")?;
    // Tee diagnostics as they arrive while retaining enough text for an
    // actionable missing-header message. Stdout/stdin remain inherited.
    let reader = std::thread::spawn(move || -> std::io::Result<Vec<u8>> {
        let mut captured = Vec::new();
        let mut buffer = [0_u8; 8192];
        let mut output = std::io::stderr().lock();
        loop {
            let n = stderr.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
            let keep = n.min((2 * 1024 * 1024_usize).saturating_sub(captured.len()));
            captured.extend_from_slice(&buffer[..keep]);
        }
        Ok(captured)
    });
    let result = child.wait();
    let stderr = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Reading compiler diagnostics failed"))??;
    Ok((result?.code().unwrap_or(1), stderr))
}

pub fn run(root: &Path, args: &[OsString], no_run: bool) -> Result<i32> {
    let state = state_directory(root)?;
    let build = state.join("build");
    let env = state.join("env");
    let metadata: Environment = serde_json::from_slice(
        &fs::read(env.join("manifest.json")).context("Run cpuv init and cpuv add first")?,
    )?;
    let path = runtime_path(root, &env, &metadata)?;
    if !compiler_command(&args[0]) {
        let mut program = PathBuf::from(&args[0]);
        if root.join(&program).is_file() {
            program = root.join(program);
        } else if program
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
        {
            let hidden = artifact(&build, program.as_os_str())?;
            if hidden.is_file() {
                program = hidden;
            }
        }
        return status(
            Command::new(program)
                .args(&args[1..])
                .current_dir(root)
                .env("PATH", path)
                .env("CPUV_ENV", env),
        );
    }
    fs::create_dir_all(build.join("tmp"))?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(build.join("build.lock"))?;
    guard
        .try_lock_exclusive()
        .context("Another build or program is using this project's build directory")?;
    let plan = plan(root, &build, &args[1..])?;
    for command in plan.commands {
        let command = compiler_args(&metadata, &env, &command)?;
        if std::env::var_os("CPUV_SHIM_TRACE").is_some() {
            eprintln!("cpuv: {:?} {:?}", metadata.compiler, command);
        }
        let (code, diagnostics) = compiler_status(
            Command::new(&metadata.compiler)
                .args(command)
                .current_dir(root)
                .env("PATH", &path)
                .env("CPUV_ENV", &env)
                .env("TMP", build.join("tmp"))
                .env("TEMP", build.join("tmp")),
        )?;
        if code != 0 {
            if let Some(help) = missing_header_help(&diagnostics) {
                eprintln!("{help}");
            }
            return Ok(code);
        } // Never execute a stale binary after failure.
    }
    if let Some(executable) = plan.executable {
        if no_run {
            eprintln!("Built {}", executable.display());
        } else {
            return status(
                Command::new(executable)
                    .args(plan.program_args)
                    .current_dir(root)
                    .env("PATH", path)
                    .env("CPUV_ENV", env),
            );
        }
    }
    Ok(0)
}

/// Archive existing loose binary artifacts without deleting user files.
pub fn clean(root: &Path) -> Result<usize> {
    let state = state_directory(root)?;
    let legacy = state.join("build/legacy");
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|s| {
                    matches!(
                        s.to_ascii_lowercase().as_str(),
                        "exe" | "dll" | "o" | "obj" | "pdb" | "ilk" | "gcno" | "gcda" | "dwo"
                    )
                })
        {
            files.push(entry.file_name());
        }
    }
    if files.is_empty() {
        eprintln!("No loose build artifacts to move");
        return Ok(0);
    }
    fs::create_dir_all(&legacy)?;
    // Persist the archive before moving anything: an error must never cause a
    // TempDir destructor to delete files already moved out of the source folder.
    let archive = tempfile::Builder::new()
        .prefix("cleanup-")
        .tempdir_in(&legacy)?
        .keep();
    fs::write(
        archive.join("files.json"),
        serde_json::to_vec_pretty(&files)?,
    )?;
    for file in &files {
        fs::rename(root.join(file), archive.join(file)).with_context(|| {
            format!(
                "Moving {} into {}",
                file.to_string_lossy(),
                archive.display()
            )
        })?;
    }
    eprintln!(
        "Moved {} build artifacts into {}",
        files.len(),
        archive.display()
    );
    Ok(files.len())
}
