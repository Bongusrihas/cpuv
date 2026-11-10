//! Explicit application assets and child-only runtime environment declarations.
use super::*;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    #[serde(default)]
    pub assets: Vec<PathBuf>,
    #[serde(default)]
    pub plugins: Vec<PathBuf>,
    #[serde(default)]
    pub search_paths: Vec<PathBuf>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl Plan {
    pub fn path(&self, root: &Path, base: &std::ffi::OsStr) -> Result<OsString> {
        self.validate()?;
        let mut paths: Vec<PathBuf> = self.search_paths.iter().map(|p| root.join(p)).collect();
        paths.extend(std::env::split_paths(base));
        Ok(std::env::join_paths(paths)?)
    }
    pub fn validate(&self) -> Result<()> {
        for path in self
            .assets
            .iter()
            .chain(&self.plugins)
            .chain(&self.search_paths)
        {
            cpuv_store::safe_relative(path)?;
        }
        let mut names = std::collections::BTreeSet::new();
        for (name, value) in &self.environment {
            ensure!(
                !name.is_empty()
                    && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    && !name.starts_with(|c: char| c.is_ascii_digit())
                    && !value.contains('\0')
                    && names.insert(name.to_ascii_uppercase()),
                "Invalid or duplicate runtime environment variable"
            );
        }
        Ok(())
    }
    pub fn variables(
        &self,
        root: &Path,
        path: &std::ffi::OsStr,
    ) -> Result<BTreeMap<String, String>> {
        self.validate()?;
        Ok(self
            .environment
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    value
                        .replace(concat!("$", "{root}"), &root.to_string_lossy())
                        .replace(concat!("$", "{path}"), &path.to_string_lossy()),
                )
            })
            .collect())
    }
}
