"""Emit a dependency/tool inventory without credentials or absolute build-machine paths.

Python 3.11+; cargo metadata and TOML readers are the authoritative manifest parsers.
This is a CycloneDX dependency inventory, not a publisher or vulnerability verdict.
"""
import argparse
import hashlib
import json
import subprocess
import tomllib
from pathlib import Path
from urllib.parse import quote


def inventory(root):
    lock_bytes = (root / "Cargo.lock").read_bytes()
    lock = tomllib.loads(lock_bytes.decode())
    bootstrap_bytes = (root / "build-tools.toml").read_bytes()
    tools = tomllib.loads(bootstrap_bytes.decode())["tools"]
    licenses = json.loads((root / "docs/tool-licenses.json").read_text(encoding="utf-8"))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--offline", "--filter-platform", "x86_64-pc-windows-msvc", "--format-version", "1"], cwd=root))
    checksums = {(p["name"], p["version"], p.get("source")): p.get("checksum") for p in lock["package"]}
    references = {}
    components = []
    for package in metadata["packages"]:
        reference = f"pkg:cargo/{quote(package['name'])}@{quote(package['version'])}"
        if reference in references.values():
            raise ValueError("Ambiguous package coordinates; record source-qualified PURLs")
        references[package["id"]] = reference
        component = {"type": "library", "bom-ref": reference, "purl": reference,
                     "name": package["name"], "version": package["version"]}
        if package.get("license"):
            component["licenses"] = [{"expression": package["license"]}]
        else:
            raise ValueError(f"Review missing license metadata for {package['name']}")
        digest = checksums.get((package["name"], package["version"], package.get("source")))
        if digest:
            component["hashes"] = [{"alg": "SHA-256", "content": digest}]
        if (package.get("repository") or "").startswith("https://"):
            component["externalReferences"] = [{"type": "vcs", "url": package["repository"]}]
        components.append(component)
    if tools.keys() != licenses.keys():
        raise ValueError("Bootstrap tool license catalog differs from build-tools.toml")
    for name, tool in tools.items():
        if not tool["url"].startswith("https://") or len(tool["sha256"]) != 64:
            raise ValueError(f"Unpinned bootstrap tool {name}")
        int(tool["sha256"], 16)
        components.append({"type": "application", "bom-ref": f"cpuv:tool:{name}:{tool['version']}",
                           "name": name, "version": tool["version"],
                           "hashes": [{"alg": "SHA-256", "content": tool["sha256"]}],
                           "licenses": [{"expression": licenses[name]["license"]}],
                           "externalReferences": [{"type": "distribution", "url": tool["url"]},
                                                  {"type": "license", "url": licenses[name]["url"]}]})
    dependencies = [{"ref": references[n["id"]], "dependsOn": sorted({references[d["pkg"]] for d in n["deps"]})}
                    for n in metadata["resolve"]["nodes"]]
    return {"bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1,
            "metadata": {"properties": [{"name": "cpuv:cargo-lock-sha256", "value": hashlib.sha256(lock_bytes).hexdigest()},
                                        {"name": "cpuv:bootstrap-manifest-sha256", "value": hashlib.sha256(bootstrap_bytes).hexdigest()},
                                        {"name": "cpuv:scope", "value": "Windows x64 Cargo graph and CPUv bootstrap tools; excludes user library graphs"}]},
            "components": sorted(components, key=lambda p: p["bom-ref"]),
            "dependencies": sorted(dependencies, key=lambda p: p["ref"])}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report = inventory(Path(__file__).resolve().parent.parent)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Recorded {len(report['components'])} components in {args.output}")
