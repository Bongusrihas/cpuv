//! A deliberately bounded pkg-config reader. Unsupported metadata fails closed.
use super::{Environment, expand, portable};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// Explicit module selection replaces automatic direct-package defaults.
    #[serde(default)]
    pub pkg_config: Vec<String>,
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

pub struct PackageFiles {
    pub name: String,
    pub files: Vec<PathBuf>,
}

#[derive(Clone)]
struct Module {
    variables: BTreeMap<String, String>,
    fields: BTreeMap<String, String>,
}

fn read(path: &Path) -> Result<Module> {
    let mut variables = BTreeMap::from([("pcfiledir".into(), portable(path.parent().unwrap()))]);
    let mut fields = BTreeMap::new();
    let text = fs::read_to_string(path)?
        .replace("\\\r\n", "")
        .replace("\\\n", "");
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if let Some((name, value)) = line.split_once('=')
            && !name.trim().contains([':', ' ', '\t'])
        {
            variables.insert(name.trim().into(), value.trim().into());
        } else if let Some((name, value)) = line.split_once(':') {
            fields.insert(name.trim().into(), value.trim().into());
        } else {
            bail!(
                "Unsupported pkg-config syntax in {}: {line}",
                path.display()
            );
        }
    }
    Ok(Module { variables, fields })
}

fn words(module: &Module, field: &str) -> Result<Vec<String>> {
    let raw = module.fields.get(field).map_or("", String::as_str);
    let tokens = shlex::split(raw).context("Invalid quoting in pkg-config metadata")?;
    let mut combined = Vec::new();
    let mut tokens = tokens.into_iter();
    while let Some(mut token) = tokens.next() {
        if ["-I", "-L", "-D"].contains(&token.as_str()) {
            token.push_str(
                &tokens
                    .next()
                    .context("Missing argument in pkg-config metadata")?,
            );
        }
        combined.push(token);
    }
    combined
        .into_iter()
        .map(|token| {
            let value = expand(&token, &module.variables);
            ensure!(
                !value.contains("${"),
                "Unresolved pkg-config variable in {value}"
            );
            Ok(value)
        })
        .collect()
}

fn requirement_modules(module: &Module, modules: &BTreeMap<String, Module>) -> Result<Vec<String>> {
    let text = expand(
        module.fields.get("Requires").map_or("", String::as_str),
        &module.variables,
    );
    let pattern =
        regex::Regex::new(r"([A-Za-z0-9_.+-]+)(?:\s*(>=|<=|==|=|>|<)\s*([A-Za-z0-9_.:+~\-]+))?")?;
    let mut output = Vec::new();
    let mut end = 0;
    for capture in pattern.captures_iter(&text) {
        let whole = capture.get(0).unwrap();
        ensure!(
            text[end..whole.start()]
                .chars()
                .all(|c| c.is_whitespace() || c == ','),
            "Unsupported pkg-config Requires syntax: {text}"
        );
        end = whole.end();
        let name = capture.get(1).unwrap().as_str();
        let operator = capture.get(2).map_or("", |value| value.as_str());
        let required = capture.get(3).map_or("", |value| value.as_str());
        let dependency = modules.get(name).with_context(|| format!("pkg-config Requires references missing module {name}; add its package to the resolved graph"))?;
        if !operator.is_empty() {
            let actual = dependency
                .fields
                .get("Version")
                .context("Required module has no Version")?;
            let order = cpuv_solver::version_cmp(&expand(actual, &dependency.variables), required);
            use std::cmp::Ordering::*;
            ensure!(
                match operator {
                    "=" | "==" => order == Equal,
                    ">=" => order != Less,
                    "<=" => order != Greater,
                    ">" => order == Greater,
                    "<" => order == Less,
                    _ => false,
                },
                "pkg-config module {name} does not satisfy {operator}{required}"
            );
        }
        output.push(name.to_owned());
    }
    ensure!(
        text[end..].chars().all(|c| c.is_whitespace() || c == ','),
        "Unsupported pkg-config Requires syntax: {text}"
    );
    Ok(output)
}

fn relative(env: &Path, value: &str) -> Result<String> {
    let value = ["/ucrt64/", "/mingw64/"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .map_or_else(|| PathBuf::from(value), |tail| env.join(tail));
    let resolved = value
        .canonicalize()
        .with_context(|| format!("Metadata directory does not exist: {}", value.display()))?;
    let root = env.canonicalize()?;
    ensure!(
        resolved.starts_with(&root),
        "Package metadata directory escapes the managed environment: {}",
        value.display()
    );
    Ok(portable(resolved.strip_prefix(root)?))
}

fn unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

pub fn apply(
    env: &Path,
    metadata: &mut Environment,
    packages: &[PackageFiles],
    roots: &[String],
    selection: &Selection,
) -> Result<()> {
    apply_for(
        env,
        metadata,
        packages,
        roots,
        selection,
        cpuv_solver::PREFIX,
    )
}

pub fn apply_for(
    env: &Path,
    metadata: &mut Environment,
    packages: &[PackageFiles],
    roots: &[String],
    selection: &Selection,
    namespace: &str,
) -> Result<()> {
    let mut modules = BTreeMap::new();
    let mut owners = BTreeMap::<String, Vec<String>>::new();
    for package in packages {
        for file in &package.files {
            if file.extension().is_some_and(|ext| ext == "pc") {
                let name = file.file_stem().unwrap().to_string_lossy().into_owned();
                ensure!(
                    !modules.contains_key(&name),
                    "Duplicate pkg-config module {name}"
                );
                modules.insert(name.clone(), read(&env.join(file))?);
                owners.entry(package.name.clone()).or_default().push(name);
            }
        }
    }
    let mut selected = selection.pkg_config.clone();
    if selected.is_empty() && selection.libraries.is_empty() && selection.flags.is_empty() {
        for root in roots {
            let name = cpuv_solver::split_requirement(root).0;
            let package_name = cpuv_solver::normalize_for(name, namespace);
            let base = package_name
                .strip_prefix(namespace)
                .unwrap_or(&package_name);
            let exported = owners.get(&package_name).cloned().unwrap_or_default();
            if exported.iter().any(|name| name == base) {
                unique(&mut selected, base.into());
                continue;
            }
            if exported.len() == 1 {
                unique(&mut selected, exported[0].clone());
                continue;
            }
            let compiled = packages
                .iter()
                .find(|p| p.name == package_name)
                .is_some_and(|p| {
                    p.files.iter().any(|file| {
                        let name = file.to_string_lossy();
                        name.ends_with(".a") || name.ends_with(".lib")
                    })
                });
            ensure!(
                !compiled,
                "Package {base} has no unambiguous link module. Set [link].pkg_config or [link].libraries explicitly; available modules: {}",
                exported.join(", ")
            );
        }
    }
    let mut pending: Vec<_> = selected.iter().rev().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let module = modules.get(&name).with_context(|| {
            format!("Selected pkg-config module {name} is not in the resolved packages")
        })?;
        pending.extend(requirement_modules(module, &modules)?.into_iter().rev());
        ordered.push(name);
        ensure!(ordered.len() <= 4096, "Too many pkg-config dependencies");
    }
    for name in &ordered {
        let module = &modules[name];
        for token in words(module, "Cflags")? {
            if let Some(path) = token.strip_prefix("-I") {
                unique(&mut metadata.include_dirs, relative(env, path)?);
            } else if let Some(define) = token.strip_prefix("-D") {
                unique(&mut metadata.defines, define.into());
            } else {
                unique(&mut metadata.cflags, token);
            }
        }
        for token in words(module, "Libs")? {
            if let Some(path) = token.strip_prefix("-L") {
                unique(&mut metadata.library_dirs, relative(env, path)?);
            } else if let Some(lib) = token.strip_prefix("-l") {
                unique(&mut metadata.libraries, lib.into());
            } else {
                unique(&mut metadata.link_flags, token);
            }
        }
    }
    for lib in &selection.libraries {
        ensure!(
            !lib.is_empty() && !lib.contains(['/', '\\', ' ', '\t']) && !lib.starts_with('-'),
            "Invalid explicit library name {lib}"
        );
        unique(&mut metadata.libraries, lib.clone());
    }
    metadata.link_flags.extend(selection.flags.clone());
    metadata.managed_binaries = packages.iter().flat_map(|p| &p.files).any(|p| {
        let name = p.to_string_lossy();
        name.ends_with(".dll") || name.ends_with(".a") || name.ends_with(".lib")
    });
    if metadata.managed_binaries {
        metadata.dependency_origin = "msys2".into();
    }
    fs::write(
        env.join("link-plan.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "authority": "resolved package ownership, pkg-config Libs/Requires, explicit project link configuration",
            "source_include_scanning": false,
            "selected_modules": selected,
            "module_graph": ordered,
            "libraries": metadata.libraries,
            "system_and_other_flags": metadata.link_flags,
            "static_private_dependencies": "not enabled; this release selects public/shared interfaces"
        }))?,
    )?;
    Ok(())
}
