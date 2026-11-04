"""Exercise live UCRT64 packages and their upstream Git repositories in isolated projects."""

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import time
import tomllib
import zipfile


PREFIX = "mingw-w64-ucrt-x86_64-"
REPOSITORY = "https://repo.msys2.org/mingw/ucrt64"
PAIR = ("zlib", "expat")
LINK = '\n[link]\npkg_config = ["zlib", "expat"]\n'
PROGRAM = '''#include <cstdio>
#include <cstring>
#include <expat.h>
#include <zlib.h>
int main() {
    const char* xml = "<cpuv>verified</cpuv>";
    auto parser = XML_ParserCreate(nullptr);
    if (!parser) return 1;
    bool parsed = XML_Parse(parser, xml, std::strlen(xml), XML_TRUE) == XML_STATUS_OK;
    XML_ParserFree(parser);
    unsigned char compressed[128];
    unsigned long size = sizeof compressed;
    if (!parsed || compress(compressed, &size,
        reinterpret_cast<const unsigned char*>(xml), std::strlen(xml)) != Z_OK) return 2;
    std::puts("CPUV_MATRIX_CONSUMER_OK");
}
'''


def write_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def sample(database, count):
    rows = {}
    with tarfile.open(database) as archive:
        for member in archive:
            if member.name.endswith("/desc"):
                sections = archive.extractfile(member).read().decode().strip().split("\n\n")
                fields = {lines[0].strip("%"): lines[1:] for section in sections
                          if (lines := section.splitlines())}
                rows[fields["NAME"][0]] = fields

    def closure(name):
        pending = [name]
        found = set()
        while pending:
            item = pending.pop()
            if item in found:
                continue
            if item not in rows:
                return None
            found.add(item)
            pending.extend(re.split(r"[<>=]", dependency, maxsplit=1)[0]
                           for dependency in rows[item].get("DEPENDS", []))
        return found

    selected = []
    repositories = set()
    # The fixed hash order spreads the sample across the catalog and is repeatable.
    for name in sorted(rows, key=lambda value: hashlib.sha256(value.encode()).digest()):
        row = rows[name]
        short = name.removeprefix(PREFIX)
        if not name.startswith(PREFIX) or short in PAIR or not re.fullmatch(r"[\w.+-]+", short):
            continue
        if short.startswith(("python-", "ruby-", "perl-", "haskell-", "nodejs-")):
            continue
        match = re.fullmatch(r"https://github\.com/([\w.-]+/[\w.-]+?)(?:\.git)?/?",
                             row.get("URL", [""])[0], re.IGNORECASE)
        if not match or match[1].lower() in repositories:
            continue
        graph = closure(name)
        if (not graph or int(row["CSIZE"][0]) > 2_500_000
                or sum(int(rows[item]["CSIZE"][0]) for item in graph) > 30_000_000
                or sum(int(rows[item]["ISIZE"][0]) for item in graph) > 100_000_000):
            continue
        repositories.add(match[1].lower())
        selected.append({"package": short, "repository": f"https://github.com/{match[1]}.git",
                         "version": row["VERSION"][0], "sha256": row["SHA256SUM"][0],
                         "archive": row["FILENAME"][0]})
        if len(selected) == count:
            return selected
    raise SystemExit(f"Only {len(selected)} distinct repositories matched the sample constraints")


def invoke(project, step, command, timeout=240, env=None):
    start = time.monotonic()
    with (project / f"{step}.log").open("wb") as log:
        try:
            child = subprocess.run(command, cwd=project, stdin=subprocess.DEVNULL, stdout=log,
                                   stderr=subprocess.STDOUT, timeout=timeout, env=env, check=False)
            code = child.returncode
        except subprocess.TimeoutExpired:
            log.write(b"\nMATRIX TIMEOUT\n")
            code = 124
    return {"exit": code, "seconds": round(time.monotonic() - start, 3)}


def git_case(case, project):
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_CONFIG_NOSYSTEM="1",
               GIT_CONFIG_GLOBAL=os.devnull)
    result = invoke(project, "git-clone", ["git", "clone", "--depth=1", "--no-tags",
                    "--no-checkout", "--", case["repository"], "upstream"], timeout=240, env=env)
    if result["exit"] == 0:
        revision = subprocess.check_output(["git", "-C", str(project / "upstream"),
                                           "rev-parse", "HEAD"], text=True, env=env).strip()
        result["revision"] = revision
    return result


def archive_case(case, project, timeout, max_mib, step="source-archive"):
    slug = case["repository"].removeprefix("https://github.com/").removesuffix(".git")
    destination = project / "upstream.zip"
    pending = project / "upstream.zip.partial"
    result = invoke(project, step, ["curl.exe", "--fail", "--silent", "--show-error",
                    "--location", "--proto", "=https", "--proto-redir", "=https", "--retry", "1",
                    "--max-time", str(timeout), "--max-filesize", str(max_mib * 1024**2),
                    "--output", str(pending), f"https://codeload.github.com/{slug}/zip/HEAD"],
                    timeout=2 * timeout + 10)
    if result["exit"] == 0:
        try:
            with zipfile.ZipFile(pending) as archive:
                revision = archive.comment.decode("ascii")
                if not re.fullmatch(r"[0-9a-f]{40}", revision) or not archive.namelist():
                    raise ValueError("source archive has no recorded commit or files")
                if sum(file.file_size for file in archive.infolist()) > 4 * max_mib * 1024**2:
                    raise ValueError("source archive exceeds the expanded verification limit")
                if archive.testzip() is not None:
                    raise ValueError("source archive CRC verification failed")
            with pending.open("rb") as payload:
                digest = hashlib.file_digest(payload, "sha256").hexdigest()
            result.update(revision=revision, sha256=digest, bytes=pending.stat().st_size)
            pending.replace(destination)
        except (ValueError, zipfile.BadZipFile) as error:
            result.update(exit=1, error=str(error))
    return result


def package_case(case, project, binary, compiler, cache, repository, adjustment=None):
    command = [str(binary), "--compiler", str(compiler), "--cache-dir", str(cache),
               "--repository", repository, "--concurrent-downloads", "4",
               "--download-timeout-secs", "100", "--max-download-mib", "64"]
    results = {"init": invoke(project, "init", command + ["init"])}
    if results["init"]["exit"]:
        return results
    (project / "main.cpp").write_text(PROGRAM, encoding="utf-8")
    manifest = project / "cpuv.toml"
    add = command + ["add", case["package"], *PAIR]
    results["auto_add"] = invoke(project, "auto-add", add, timeout=600)
    last_add = results["auto_add"]
    if (last_add["exit"] and adjustment
            and adjustment["when"] in (project / "auto-add.log").read_text(errors="replace")):
        add = command + ["add", *adjustment["packages"]]
        results["adjusted_add"] = last_add = invoke(project, "adjusted-add", add, timeout=600)
    if last_add["exit"]:
        original = manifest.read_text(encoding="utf-8")
        if "\n[link]" in original:
            raise ValueError(f"Unexpected link section in initial manifest: {project}")
        manifest.write_text(original + LINK, encoding="utf-8")
        results["explicit_add"] = invoke(project, "explicit-add", add, timeout=600)
        if results["explicit_add"]["exit"]:
            return results
    lock = tomllib.loads((project / "cpuv.lock").read_text(encoding="utf-8"))
    results["provider"] = lock.get("provider", "msys2")
    if results["provider"] != "msys2":
        return results
    payload = tomllib.loads(lock["payload"]) if "payload" in lock else lock
    locked = next((item for item in payload.get("packages", [])
                   if item["name"] == PREFIX + case["package"]), None)
    if locked:
        archive = cache / "archives" / (locked["sha256"] + ".pkg.tar.zst")
        results["locked_package"] = {"version": locked["version"], "archive": locked["filename"],
                                     "sha256": locked["sha256"],
                                     "archive_verified": archive.is_file() and
                                     hashlib.sha256(archive.read_bytes()).hexdigest() == locked["sha256"]}
    results["lock_size"] = len(payload.get("packages", []))
    results["verify"] = invoke(project, "verify", command + ["--offline", "verify"])
    if results["verify"]["exit"] == 0:
        results["run"] = invoke(project, "run", command + ["--offline", "run", "g++", "main.cpp"])
        results["run"]["observed_output"] = b"CPUV_MATRIX_CONSUMER_OK" in (project / "run.log").read_bytes()
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--database", type=Path, required=True,
                        help="UCRT64 database snapshot used for the sample")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--compiler", type=Path)
    parser.add_argument("--repository", default=REPOSITORY, help="HTTPS UCRT64 repository URL")
    parser.add_argument("--cache-dir", type=Path, help="Share the package cache with another matrix run")
    parser.add_argument("--adjustments", type=Path, help="TOML test inputs for explicitly corrected package requests")
    parser.add_argument("--count", type=int, default=200)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--mode", choices=("git", "archive", "package"), required=True)
    parser.add_argument("--source-timeout", type=int, default=75, help="Source ZIP transfer deadline in seconds")
    parser.add_argument("--source-max-mib", type=int, default=64, help="Compressed source ZIP size limit")
    parser.add_argument("--retry-archives", action="store_true", help="Retry failed ZIPs and preserve first-pass results")
    args = parser.parse_args()
    if min(args.count, args.workers, args.source_timeout, args.source_max_mib) < 1:
        parser.error("count, workers and source limits must be positive")
    if args.retry_archives and args.mode != "archive":
        parser.error("--retry-archives requires archive mode")
    if args.mode == "package" and (not args.binary or not args.compiler):
        parser.error("package mode requires --binary and --compiler")
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    selected = sample(args.database, args.count)
    cache = args.cache_dir.resolve() if args.cache_dir else root / "cache"
    adjustments = tomllib.loads(args.adjustments.read_text(encoding="utf-8"))["adjustments"] if args.adjustments else {}

    def run(index, case):
        project = root / f"{index:03}-{case['package']}"
        project.mkdir(exist_ok=True)
        result_path = project / f"{args.mode}-result.json"
        if result_path.exists():
            previous = json.loads(result_path.read_text(encoding="utf-8"))
            if previous["case"] != case:
                raise ValueError(f"The saved case does not match the requested snapshot: {project}")
            if not args.retry_archives or previous["result"]["exit"] == 0:
                return previous
            result = archive_case(case, project, args.source_timeout, args.source_max_mib,
                                  step="source-archive-retry")
            row = {"case": case, "project": project.name, "initial": previous["result"], "result": result}
            write_json(result_path, row)
            return row
        if args.mode == "git":
            result = git_case(case, project)
        elif args.mode == "archive":
            result = archive_case(case, project, args.source_timeout, args.source_max_mib)
        else:
            result = package_case(case, project, args.binary.resolve(), args.compiler.resolve(),
                                  cache, args.repository, adjustments.get(case["package"]))
        row = {"case": case, "project": project.name, "result": result}
        write_json(result_path, row)
        return row

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = [executor.submit(run, index, case) for index, case in enumerate(selected, 1)]
        for completed, future in enumerate(concurrent.futures.as_completed(futures), 1):
            row = future.result()
            status = (row["result"]["exit"] if args.mode != "package"
                      else {key: step["exit"] for key, step in row["result"].items()
                            if isinstance(step, dict) and "exit" in step})
            print(f"{completed}/{len(selected)} {row['project']} {status}", flush=True)
    rows = [future.result() for future in futures]
    write_json(root / f"{args.mode}-results.json", rows)
    if args.mode == "package":
        passed = sum(row["result"].get("run", {}).get("exit") == 0
                     and row["result"]["run"].get("observed_output", False)
                     and row["result"].get("locked_package", {}).get("archive_verified", False)
                     for row in rows)
    else:
        passed = sum(row["result"]["exit"] == 0 for row in rows)
    print(f"{args.mode}: {passed}/{len(rows)} passed", flush=True)
    return 0 if passed == len(rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
