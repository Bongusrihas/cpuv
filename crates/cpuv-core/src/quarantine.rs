//! Preserve damaged CPUv-owned cache objects before attempting reconstruction.
use super::*;

pub fn preserve(path: &Path, parent: &Path) -> Result<PathBuf> {
    no_reparse(path)?;
    fs::create_dir_all(parent)?;
    no_reparse(parent)?;
    let destination = tempfile::Builder::new()
        .prefix("damaged-")
        .tempdir_in(parent)?
        .keep();
    let destination = destination.join(path.file_name().context("Cache object has no name")?);
    fs::rename(path, &destination)
        .with_context(|| format!("Preserving damaged cache object {}", path.display()))?;
    eprintln!(
        "Preserved damaged cache object at {}",
        destination.display()
    );
    Ok(destination)
}
