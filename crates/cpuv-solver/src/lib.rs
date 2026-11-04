//! MSYS2 publishes one version per package in a repository snapshot. Resolve that
//! snapshot deterministically and validate every constraint; no pretend SemVer
//! conversion of pacman versions and no silent dropping of runtime dependencies.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

pub const PREFIX: &str = "mingw-w64-ucrt-x86_64-";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub version: String,
    pub filename: String,
    pub sha256: String,
    pub size: u64,
    pub depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
}

pub type Index = BTreeMap<String, Package>;

pub fn parse_desc(text: &str) -> Result<Package> {
    let mut fields: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut key = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('%') && line.ends_with('%') {
            key = line.trim_matches('%');
        } else if !line.is_empty() {
            fields.entry(key).or_default().push(line);
        }
    }
    let one = |key| -> Result<String> {
        fields
            .get(key)
            .and_then(|v| v.first())
            .map(|s| s.to_string())
            .with_context(|| format!("Missing %{key}% in repository entry"))
    };
    let many = |key| -> Vec<String> {
        fields
            .get(key)
            .map(|v| v.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    };
    let p = Package {
        name: one("NAME")?,
        base: fields
            .get("BASE")
            .and_then(|v| v.first())
            .map(|s| s.to_string()),
        version: one("VERSION")?,
        filename: one("FILENAME")?,
        sha256: one("SHA256SUM")?.to_ascii_lowercase(),
        size: one("CSIZE")?.parse()?,
        depends: many("DEPENDS"),
        provides: many("PROVIDES"),
        conflicts: many("CONFLICTS"),
    };
    ensure!(
        p.name.starts_with("mingw-w64-"),
        "Non-MinGW package in index: {}",
        p.name
    );
    ensure!(
        p.sha256.len() == 64 && p.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256 for {}",
        p.name
    );
    ensure!(
        !p.filename.contains(['/', '\\', ':']) && p.filename.ends_with(".pkg.tar.zst"),
        "Unsafe package filename {}",
        p.filename
    );
    Ok(p)
}

pub fn normalize(name: &str) -> String {
    normalize_for(name, PREFIX)
}

/// MSYS2 split packages from a compiler recipe share its build version.
pub fn compiler_build_mismatch<'a>(
    packages: &'a [Package],
    compiler_family: &str,
    compiler_major: &str,
) -> Option<&'a Package> {
    let compiler_base = format!("mingw-w64-{compiler_family}");
    packages.iter().find(|package| {
        package.base.as_deref() == Some(compiler_base.as_str())
            && package
                .version
                .rsplit(':')
                .next()
                .unwrap_or_default()
                .split(['.', '-'])
                .next()
                != Some(compiler_major)
    })
}

pub fn normalize_for(name: &str, prefix: &str) -> String {
    if name.starts_with("mingw-w64-") {
        return name.to_owned();
    }
    format!("{prefix}{name}")
}

pub fn split_requirement(text: &str) -> (&str, &str, &str) {
    let n = text.find(['<', '>', '=']).unwrap_or(text.len());
    let (name, rest) = text.split_at(n);
    for op in [">=", "<=", "==", "=", ">", "<"] {
        if let Some(version) = rest.strip_prefix(op) {
            return (name, op, version);
        }
    }
    (name, "", "")
}

// Pacman epoch, numeric/alphabetic segments, and package release ordering.
// A missing pkgrel is deliberately ignored, matching libalpm's comparison rule.
fn segments_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.as_bytes();
    let mut b = b.as_bytes();
    loop {
        if a.is_empty() && b.is_empty() {
            return Ordering::Equal;
        }
        if a.is_empty() {
            return if b[0].is_ascii_alphabetic() {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        if b.is_empty() {
            return if a[0].is_ascii_alphabetic() {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let start_a = a.len();
        let start_b = b.len();
        while !a.is_empty() && !a[0].is_ascii_alphanumeric() {
            a = &a[1..];
        }
        while !b.is_empty() && !b[0].is_ascii_alphanumeric() {
            b = &b[1..];
        }
        let separators = (start_a - a.len()).cmp(&(start_b - b.len()));
        if separators != Ordering::Equal {
            return separators;
        }
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let numeric_a = a[0].is_ascii_digit();
        let numeric_b = b[0].is_ascii_digit();
        if numeric_a != numeric_b {
            return numeric_a.cmp(&numeric_b);
        }
        let len_a = a
            .iter()
            .take_while(|x| {
                if numeric_a {
                    x.is_ascii_digit()
                } else {
                    x.is_ascii_alphabetic()
                }
            })
            .count();
        let len_b = b
            .iter()
            .take_while(|x| {
                if numeric_b {
                    x.is_ascii_digit()
                } else {
                    x.is_ascii_alphabetic()
                }
            })
            .count();
        let (mut x, mut y) = (&a[..len_a], &b[..len_b]);
        if numeric_a {
            while x.first() == Some(&b'0') {
                x = &x[1..];
            }
            while y.first() == Some(&b'0') {
                y = &y[1..];
            }
        }
        let order = if numeric_a {
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        } else {
            x.cmp(y)
        };
        if order != Ordering::Equal {
            return order;
        }
        a = &a[len_a..];
        b = &b[len_b..];
    }
}

pub fn version_cmp(a: &str, b: &str) -> Ordering {
    fn epoch(s: &str) -> (u64, &str) {
        s.split_once(':')
            .map_or((0, s), |(e, v)| (e.parse::<u64>().unwrap_or(0), v))
    }
    let (ea, a) = epoch(a);
    let (eb, b) = epoch(b);
    if ea != eb {
        return ea.cmp(&eb);
    }
    let (va, ra) = a.rsplit_once('-').map_or((a, None), |(v, r)| (v, Some(r)));
    let (vb, rb) = b.rsplit_once('-').map_or((b, None), |(v, r)| (v, Some(r)));
    segments_cmp(va, vb).then_with(|| match (ra, rb) {
        (Some(x), Some(y)) => segments_cmp(x, y),
        _ => Ordering::Equal,
    })
}

fn matches(version: &str, op: &str, required: &str) -> bool {
    let order = version_cmp(version, required);
    match op {
        "" => true,
        "=" | "==" => order == Ordering::Equal,
        ">=" => order != Ordering::Less,
        "<=" => order != Ordering::Greater,
        ">" => order == Ordering::Greater,
        "<" => order == Ordering::Less,
        _ => false,
    }
}

/// Rank names from the actual registry; suggestions never select a dependency.
pub fn suggestions<'a>(query: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    fn distance(a: &str, b: &str) -> usize {
        let b: Vec<_> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut diagonal = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let old = row[j + 1];
                row[j + 1] = (row[j] + 1)
                    .min(old + 1)
                    .min(diagonal + usize::from(ca != *cb));
                diagonal = old;
            }
        }
        row[b.len()]
    }
    let query = query.to_ascii_lowercase();
    if query.is_empty() || query.len() > 256 {
        return Vec::new();
    }
    let threshold = (query.len() / 3).clamp(1, 4);
    let mut ranked = BTreeSet::new();
    for name in names {
        if name.len() > 256 {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let contained =
            query.len().min(lower.len()) >= 3 && (lower.contains(&query) || query.contains(&lower));
        if !contained && query.len().abs_diff(lower.len()) > threshold {
            continue;
        }
        let score = distance(&query, &lower);
        if contained || score <= threshold {
            ranked.insert((usize::from(!contained), score, name.to_owned()));
        }
    }
    ranked
        .into_iter()
        .take(5)
        .map(|(_, _, name)| name)
        .collect()
}

type Providers<'a> = BTreeMap<&'a str, Vec<&'a Package>>;

fn providers(index: &Index) -> Providers<'_> {
    let mut by_name = Providers::new();
    for package in index.values() {
        let mut seen = BTreeSet::new();
        for provided in &package.provides {
            let name = split_requirement(provided).0;
            if seen.insert(name) {
                by_name.entry(name).or_default().push(package);
            }
        }
    }
    by_name
}

fn provider<'a>(
    index: &'a Index,
    providers: &Providers<'a>,
    requirement: &str,
    prefix: &str,
) -> Result<&'a Package> {
    let (name, op, version) = split_requirement(requirement);
    ensure!(
        op.is_empty() || (!version.is_empty() && !version.contains(['<', '>', '='])),
        "Invalid version constraint {requirement}"
    );
    if let Some(p) = index.get(name) {
        ensure!(
            matches(&p.version, op, version),
            "Unsatisfied constraint {requirement}; repository contains {} {}",
            p.name,
            p.version
        );
        return Ok(p);
    }
    let candidates: Vec<_> = providers
        .get(name)
        .into_iter()
        .flatten()
        .copied()
        .filter(|p| {
            p.provides.iter().any(|s| {
                let (n, _, v) = split_requirement(s);
                n == name && (op.is_empty() || (!v.is_empty() && matches(v, op, version)))
            })
        })
        .collect();
    match candidates.as_slice() {
        [p] => Ok(p),
        [] => {
            let similar = suggestions(
                name.strip_prefix(prefix).unwrap_or(name),
                index.keys().filter_map(|name| name.strip_prefix(prefix)),
            );
            bail!(
                "No package in the selected repository provides {requirement}. {}",
                if similar.is_empty() {
                    "No similar package names were found in this snapshot".into()
                } else {
                    format!(
                        "Similar packages: {}. Use the exact repository name with cpuv add",
                        similar.join(", ")
                    )
                }
            )
        }
        _ => bail!(
            "Ambiguous providers for {requirement}: {}",
            candidates
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn resolve(index: &Index, roots: &[String]) -> Result<Vec<Package>> {
    resolve_for(index, roots, PREFIX)
}

/// Resolve public request identities to package owners for consumer integration.
pub fn root_packages_for(index: &Index, roots: &[String], prefix: &str) -> Result<Vec<String>> {
    let providers = providers(index);
    roots
        .iter()
        .map(|root| {
            let (name, op, version) = split_requirement(root);
            Ok(provider(
                index,
                &providers,
                &format!("{}{op}{version}", normalize_for(name, prefix)),
                prefix,
            )?
            .name
            .clone())
        })
        .collect()
}

pub fn available(index: &Index, name: &str, prefix: &str) -> bool {
    let normalized = normalize_for(name, prefix);
    index.contains_key(&normalized)
        || index.values().any(|package| {
            package
                .provides
                .iter()
                .any(|provided| split_requirement(provided).0 == normalized)
        })
}

pub fn resolve_for(index: &Index, roots: &[String], prefix: &str) -> Result<Vec<Package>> {
    fn visit(
        index: &Index,
        providers: &Providers<'_>,
        req: &str,
        selected: &mut BTreeMap<String, Package>,
        visiting: &mut BTreeSet<String>,
        ordered: &mut Vec<Package>,
        prefix: &str,
    ) -> Result<()> {
        let p = provider(index, providers, req, prefix)?;
        if selected.contains_key(&p.name) || visiting.contains(&p.name) {
            return Ok(());
        }
        ensure!(
            visiting.len() < 128,
            "Dependency graph exceeds maximum depth of 128 at {}",
            p.name
        );
        visiting.insert(p.name.clone());
        for dep in &p.depends {
            visit(index, providers, dep, selected, visiting, ordered, prefix)
                .with_context(|| format!("Required by {} {}", p.name, p.version))?;
        }
        visiting.remove(&p.name);
        selected.insert(p.name.clone(), p.clone());
        ordered.push(p.clone());
        Ok(())
    }
    let mut selected = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    let mut ordered = Vec::new();
    let providers = providers(index);
    for root in roots {
        let (n, o, v) = split_requirement(root);
        let normalized = normalize_for(n, prefix);
        visit(
            index,
            &providers,
            &format!("{normalized}{o}{v}"),
            &mut selected,
            &mut visiting,
            &mut ordered,
            prefix,
        )?;
    }
    for p in selected.values() {
        for conflict in &p.conflicts {
            let (name, op, version) = split_requirement(conflict);
            ensure!(
                op.is_empty() || (!version.is_empty() && !version.contains(['<', '>', '='])),
                "Invalid conflict constraint {conflict}"
            );
            for other in selected.values().filter(|other| other.name != p.name) {
                let conflicts = (other.name == name && matches(&other.version, op, version))
                    || other.provides.iter().any(|provided| {
                        let (identity, _, provided_version) = split_requirement(provided);
                        identity == name
                            && (op.is_empty()
                                || (!provided_version.is_empty()
                                    && matches(provided_version, op, version)))
                    });
                ensure!(
                    !conflicts,
                    "Conflicting packages: {} conflicts with {}",
                    p.name,
                    other.name
                );
            }
        }
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Manual same-process synthetic provider lookup benchmark"]
    fn provider_lookup_benchmark() {
        let index: Index = (0..3000)
            .map(|i| {
                let p = package(&format!("p{i:04}"), &[&format!("virtual-{i}=1")], &[]);
                (p.name.clone(), p)
            })
            .collect();
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let reference: Vec<_> = (0..500)
                .map(|i| {
                    let name = format!("virtual-{i}");
                    index
                        .values()
                        .find(|p| p.provides.iter().any(|s| split_requirement(s).0 == name))
                        .unwrap()
                        .name
                        .clone()
                })
                .collect();
            let reference_time = start.elapsed();
            let start = std::time::Instant::now();
            let by_name = providers(&index);
            let indexed: Vec<_> = (0..500)
                .map(|i| {
                    provider(&index, &by_name, &format!("virtual-{i}>=1"), "")
                        .unwrap()
                        .name
                        .clone()
                })
                .collect();
            let indexed_time = start.elapsed();
            assert_eq!(reference, indexed);
            eprintln!(
                "provider_lookup packages=3000 requests=500 reference_us={} indexed_including_construction_us={}",
                reference_time.as_micros(),
                indexed_time.as_micros()
            );
        }
    }
    #[test]
    fn deep_dependency_graphs_fail_without_stack_overflow() {
        let mut index = Index::new();
        for i in 0..160 {
            let mut p = package(&format!("p{i}"), &[], &[]);
            if i < 159 {
                p.depends.push(format!("p{}", i + 1));
            }
            index.insert(p.name.clone(), p);
        }
        assert!(
            format!("{:#}", resolve_for(&index, &["p0".into()], "").unwrap_err())
                .contains("maximum depth")
        );
    }
    #[test]
    fn duplicate_virtual_declarations_do_not_create_ambiguity() {
        let index = BTreeMap::from([("a".into(), package("a", &["virtual=2", "virtual=2"], &[]))]);
        assert_eq!(
            resolve_for(&index, &["virtual>=2".into()], "").unwrap()[0].name,
            "a"
        );
        assert!(resolve_for(&index, &["virtual>=3".into()], "").is_err());
    }
    fn package(name: &str, provides: &[&str], conflicts: &[&str]) -> Package {
        Package {
            name: name.into(),
            base: None,
            version: "1".into(),
            filename: "test.pkg.tar.zst".into(),
            sha256: "0".repeat(64),
            size: 0,
            depends: vec![],
            provides: provides.iter().map(|s| s.to_string()).collect(),
            conflicts: conflicts.iter().map(|s| s.to_string()).collect(),
        }
    }
    #[test]
    fn compiler_release_guard_uses_the_recipe_base_and_ignores_the_epoch() {
        let mut split = package("mingw-w64-arbitrary-split-name", &[], &[]);
        split.base = Some("mingw-w64-example-compiler".into());
        split.version = "2:17.3.0-1".into();
        let mut independent = package("mingw-w64-unrelated", &[], &[]);
        independent.base = Some("mingw-w64-another-recipe".into());
        let legacy = package("mingw-w64-legacy-entry", &[], &[]);
        let packages = [independent, legacy, split];
        assert!(compiler_build_mismatch(&packages, "example-compiler", "17").is_none());
        assert_eq!(
            compiler_build_mismatch(&packages, "example-compiler", "16"),
            Some(&packages[2])
        );
    }
    #[test]
    fn ambiguous_virtual_providers_do_not_hide_conflicts() {
        let index: Index = [
            package("a", &["virtual"], &[]),
            package("b", &["virtual"], &[]),
            package("p", &[], &["virtual"]),
        ]
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect();
        assert!(resolve_for(&index, &["a".into(), "b".into(), "p".into()], "").is_err());
    }
    #[test]
    fn versioned_conflict_only_matches_compatible_provides() {
        let index: Index = [
            package("a", &["virtual=2"], &[]),
            package("p", &[], &["virtual>=3"]),
        ]
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect();
        assert!(resolve_for(&index, &["a".into(), "p".into()], "").is_ok());
    }
    #[test]
    fn virtual_root_maps_to_selected_package_owner() {
        let p = package("implementation", &["virtual=2"], &[]);
        let index = Index::from([(p.name.clone(), p)]);
        assert_eq!(
            root_packages_for(&index, &["virtual>=2".into()], "").unwrap(),
            ["implementation"]
        );
    }
}
