//! Common portable envelope; provider-native payloads remain authoritative.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    provider: String,
    request_hash: String,
    payload_blake3: String,
    dependencies: toml::Table,
    payload: String,
}

pub fn decode(text: &str) -> Result<String> {
    let header: toml::Value = toml::from_str(text).context("Invalid dependency lock")?;
    let version = header
        .get("version")
        .and_then(toml::Value::as_integer)
        .context("Lock has no version")?;
    if (1..=4).contains(&version) {
        return Ok(text.to_owned());
    }
    ensure!(
        version == 5,
        "Unsupported dependency lock version {version}"
    );
    let envelope: Envelope = toml::from_str(text)?;
    ensure!(
        blake3::hash(envelope.payload.as_bytes()).to_hex().as_str() == envelope.payload_blake3,
        "Dependency lock payload changed without a matching receipt"
    );
    let payload: toml::Value = toml::from_str(&envelope.payload)?;
    let dependencies = payload.get("dependencies").and_then(toml::Value::as_table);
    ensure!(
        dependencies == Some(&envelope.dependencies),
        "Lock envelope dependencies disagree with its provider payload"
    );
    let provider = payload
        .get("backend")
        .and_then(toml::Value::as_str)
        .unwrap_or("msys2");
    ensure!(
        provider == envelope.provider,
        "Lock provider disagrees with its payload"
    );
    let inner = payload
        .get("version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    ensure!(
        (1..=4).contains(&inner),
        "Unsupported provider lock payload"
    );
    Ok(envelope.payload)
}

pub fn read(path: &Path) -> std::io::Result<String> {
    let text = fs::read_to_string(path)?;
    decode(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e:#}")))
}

/// Formatting and envelope migrations do not change a provider resolution's identity.
pub fn identity(payload: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(payload).context("Invalid provider lock")?;
    Ok(blake3::hash(&serde_json::to_vec(&value)?)
        .to_hex()
        .to_string())
}

pub fn verify_identity(root: &Path, installed: Option<&str>) -> Result<()> {
    match read(&root.join("cpuv.lock")) {
        Ok(payload) => ensure!(
            installed == Some(identity(&payload)?.as_str()),
            "Installed environment does not match cpuv.lock, or needs migration; run cpuv sync before building or exporting"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ensure!(
            installed.is_none(),
            "Installed dependency lock is missing; restore cpuv.lock and run cpuv sync"
        ),
        Err(error) => return Err(error).context("Validating installed dependency lock"),
    }
    Ok(())
}

pub fn write(root: &Path, destination: &Path, payload: &str, request_hash: &str) -> Result<()> {
    let value: toml::Value = toml::from_str(payload)?;
    let provider = value
        .get("backend")
        .and_then(toml::Value::as_str)
        .unwrap_or("msys2")
        .to_owned();
    ensure!(
        ["msys2", "conan", "vcpkg"].contains(&provider.as_str()),
        "Unknown lock provider"
    );
    let dependencies = value
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    let previous = root.join("cpuv.lock");
    if previous.is_file() {
        let bytes = fs::read(&previous)?;
        let parent = root.join(".cpuv/lock-backups");
        fs::create_dir_all(&parent)?;
        let backup = parent.join(format!("{}.toml", blake3::hash(&bytes).to_hex()));
        if !backup.exists() {
            atomic_write(&backup, &bytes)?;
        }
    }
    let envelope = Envelope {
        version: 5,
        provider,
        request_hash: request_hash.into(),
        payload_blake3: blake3::hash(payload.as_bytes()).to_hex().to_string(),
        dependencies,
        payload: payload.into(),
    };
    atomic_write(destination, toml::to_string_pretty(&envelope)?.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_identity_rejects_changed_missing_and_legacy_locks() {
        let root = tempfile::tempdir().unwrap();
        let a = "version=1\npackages=[]\n[dependencies]\n";
        let b = "version=1\nrepository='different'\npackages=[]\n[dependencies]\n";
        let identity = identity(a).unwrap();
        verify_identity(root.path(), None).unwrap();
        assert!(verify_identity(root.path(), Some(&identity)).is_err());
        fs::write(root.path().join("cpuv.lock"), a).unwrap();
        verify_identity(root.path(), Some(&identity)).unwrap();
        assert!(verify_identity(root.path(), None).is_err());
        fs::write(root.path().join("cpuv.lock"), b).unwrap();
        assert!(verify_identity(root.path(), Some(&identity)).is_err());
        assert_eq!(
            identity,
            super::identity("packages = []\nversion = 1\n[dependencies]\n").unwrap()
        );
    }
    #[test]
    fn locks_round_trip_and_reject_mismatched_or_future_payloads() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".cpuv")).unwrap();
        let payload = "version=3\nbackend=\"conan\"\n[dependencies]\nfixture=\"1\"\n";
        let path = root.path().join("cpuv.lock");
        write(root.path(), &path, payload, "request").unwrap();
        assert_eq!(read(&path).unwrap(), payload);
        let text = fs::read_to_string(&path).unwrap();
        assert!(decode(&text.replace("fixture = \"1\"", "fixture = \"2\"")).is_err());
        assert!(decode("version=99").is_err());
        write(root.path(), &path, payload, "request").unwrap();
        assert_eq!(
            fs::read_dir(root.path().join(".cpuv/lock-backups"))
                .unwrap()
                .count(),
            1
        );
    }
}
