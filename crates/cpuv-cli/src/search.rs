//! Provider-qualified discovery; search results do not imply binary compatibility.
use super::*;

pub async fn execute(root: &Path, cli: &Cli, provider: &str, query: &str) -> Result<i32> {
    ensure!(
        !query.is_empty()
            && query
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.+*".contains(&b)),
        "Search with a package name or wildcard"
    );
    match provider {
        "conan" => source::search(root, cli, query).await?,
        "vcpkg" => vcpkg::search(root, cli, query)?,
        "msys2" => {
            let tc = cpuv_toolchain::probe(cli.compiler.as_deref())?;
            let provider=tc.provider.context("This compiler has no compatible MSYS2 repository; search Conan or vcpkg explicitly")?;
            let repository = cli.repository.clone().unwrap_or(provider.repository);
            let fetcher = Fetcher::with_limits(
                repository.clone(),
                cache_dir(cli)?,
                cli.concurrent_downloads as usize,
                cli.offline,
                cli.transfer_limits(),
            )?;
            let index = fetcher.index(cli.refresh).await?;
            let needle = query.trim_matches('*');
            let matches: Vec<_> = index.values().filter(|p| p.name.contains(needle)).collect();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"provider":"msys2","repository":repository,"packages":matches,
                "note":"Use the exact provider name. Availability does not establish compiler/ABI compatibility."})
                )?
            );
        }
        _ => bail!("Choose msys2, conan or vcpkg"),
    }
    Ok(0)
}
