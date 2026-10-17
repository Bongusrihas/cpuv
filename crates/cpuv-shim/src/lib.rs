use anyhow::{Context, Result, bail, ensure};
use cpuv_relocate::{Environment, compiler_args};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn environment() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    if let Some(cpuv) = exe.parent().and_then(Path::parent) {
        let env = cpuv.join("env");
        if env.join("manifest.json").is_file() {
            return Ok(env);
        }
    }
    for dir in std::env::current_dir()?.ancestors() {
        let env = dir.join(".cpuv/env");
        if env.join("manifest.json").is_file() {
            return Ok(env);
        }
    }
    bail!("No cpuv environment; run cpuv init and cpuv add first")
}

fn output_path(args: &[OsString]) -> Option<PathBuf> {
    if args.is_empty()
        || args.iter().any(|s| {
            let s = s.to_string_lossy();
            matches!(
                s.as_ref(),
                "-c" | "-E"
                    | "-S"
                    | "-M"
                    | "-MM"
                    | "-fsyntax-only"
                    | "--version"
                    | "--help"
                    | "-###"
            ) || s.starts_with("-print-")
                || s.starts_with("-dump")
                || s == "-v" && args.len() == 1
        })
    {
        return None;
    }
    for (i, arg) in args.iter().enumerate() {
        if arg == "-o" {
            return args.get(i + 1).map(PathBuf::from);
        }
        if let Some(out) = arg.to_str().and_then(|s| s.strip_prefix("-o"))
            && !out.is_empty()
        {
            return Some(out.into());
        }
    }
    Some("a.exe".into())
}

fn deploy_dlls(binary: &Path, env: &Path, compiler_bin: &Path) -> Result<()> {
    if !binary.is_file() {
        return Ok(());
    }
    let output = binary
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut pending = vec![binary.to_owned()];
    let mut seen = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let bytes = fs::read(&path)?;
        let pe = match goblin::Object::parse(&bytes)? {
            goblin::Object::PE(pe) => pe,
            _ => continue,
        };
        for library in pe.libraries {
            let key = library.to_ascii_lowercase();
            if !seen.insert(key.clone()) {
                continue;
            }
            ensure!(
                Path::new(library).file_name().is_some_and(|s| s == library)
                    && !library.contains(['/', '\\', ':']),
                "Unsafe PE import name"
            );
            let source = [env.join("bin"), compiler_bin.to_owned()]
                .into_iter()
                .find_map(|dir| {
                    let candidate = dir.join(library);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                    fs::read_dir(dir)
                        .ok()?
                        .filter_map(|e| e.ok())
                        .find(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .eq_ignore_ascii_case(library)
                        })
                        .map(|e| e.path())
                });
            // Imports absent from both package and compiler bins are OS or user DLLs.
            if let Some(source) = source {
                let dest = output.join(library);
                if dest.exists() {
                    ensure!(
                        fs::read(&dest)? == fs::read(&source)?,
                        "{} already contains a different DLL; remove it or use a fresh output directory",
                        dest.display()
                    );
                } else if fs::hard_link(&source, &dest).is_err() {
                    fs::copy(&source, &dest)?;
                }
                pending.push(source);
            }
        }
    }
    Ok(())
}

pub fn run() -> Result<i32> {
    let env = environment()?;
    let metadata: Environment = serde_json::from_slice(&fs::read(env.join("manifest.json"))?)?;
    let input: Vec<OsString> = std::env::args_os().skip(1).collect();
    ensure!(
        !input.iter().any(|s| s.to_string_lossy().starts_with('@')),
        "Phase A shim does not yet support response files; pass compiler arguments directly"
    );
    let args = compiler_args(&metadata, &env, &input)?;
    if std::env::var_os("CPUV_SHIM_TRACE").is_some() {
        eprintln!("cpuv-shim: {:?} {:?}", metadata.compiler, args);
    }
    let mut paths = vec![env.join("bin"), metadata.compiler_bin.clone()];
    if let Some(host) = cpuv_relocate::msys_host_bin(&metadata.compiler_bin) {
        paths.push(host);
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    let status = Command::new(&metadata.compiler)
        .args(args)
        .env("PATH", std::env::join_paths(paths)?)
        .status()
        .context("Launching the selected GCC compiler")?;
    if status.success()
        && let Some(mut output) = output_path(&input)
    {
        if !output.is_file() {
            let mut with_exe = output.as_os_str().to_owned();
            with_exe.push(".exe");
            if Path::new(&with_exe).is_file() {
                output = with_exe.into();
            }
        }
        deploy_dlls(&output, &env, &metadata.compiler_bin)?;
    }
    Ok(status.code().unwrap_or(1))
}
