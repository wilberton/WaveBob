"""Create a native development release kit with complete relinking inputs.

Run with Python 3.11+, Rust, native C build tools and rust-src installed.
Vendored dependencies must match Cargo.lock. No signing or publishing occurs.
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import tempfile
import tomllib
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--output", type=pathlib.Path, default=ROOT / "dist")
parser.add_argument("--vendor", type=pathlib.Path)
args = parser.parse_args()


def run(*command, capture=False):
    return subprocess.run(command, cwd=ROOT, check=True, text=True,
                          stdout=subprocess.PIPE if capture else None).stdout


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
rust = run("rustc", "--version", "--verbose", capture=True)
rust_fields = dict(line.split(": ", 1) for line in rust.splitlines()[1:] if ": " in line)
host = rust_fields["host"]
suffix = ".exe" if "windows" in host else ""
commit = run("git", "rev-parse", "HEAD", capture=True).strip()
assert not run("git", "status", "--porcelain", capture=True).strip(), "Commit source changes before packaging"
name = f"WaveBob-{version}-{host}"
args.output = args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)
archive = args.output / f"{name}.zip"
assert not archive.exists(), f"Refusing to replace {archive}"
sysroot = pathlib.Path(run("rustc", "--print", "sysroot", capture=True).strip())
rust_source = sysroot / "lib/rustlib/src/rust"
assert rust_source.is_dir(), "Install rustup component add rust-src first"

# Build separately: automation executables must not inherit the GUI feature.
run("cargo", "build", "--locked", "--release", "--bin", "wavebob")
run("cargo", "build", "--locked", "--release", "--no-default-features",
    "--bin", "wavebob-headless", "--bin", "wavebob-mcp", "--example", "workflow")

with tempfile.TemporaryDirectory(prefix=".wavebob-release-", dir=args.output) as temporary:
    temporary = pathlib.Path(temporary)
    vendor = args.vendor.resolve() if args.vendor else temporary / "vendor"
    if args.vendor is None:
        run("cargo", "vendor", "--locked", str(vendor), capture=True)
    assert (vendor / "mp3lame-sys/lame-3.100/COPYING").is_file(), "Incomplete encoder source kit"
    locked = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    packages = []
    for manifest in sorted(vendor.glob("*/Cargo.toml")):
        package = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
        checksum = json.loads((manifest.parent / ".cargo-checksum.json").read_text())["package"]
        assert any(item["name"] == package["name"] and item["version"] == package["version"]
                   and item.get("checksum") == checksum for item in locked), f"Wrong vendor version: {manifest}"
        packages.append({"name": package["name"], "version": package["version"],
                         "license": package.get("license", package.get("license-file", "See source notices")),
                         "source": f"source/vendor/{manifest.parent.name}", "registry_sha256": checksum})
    assert len(packages) == len([item for item in locked if "source" in item]), "Vendor set is incomplete"
    notices = ["# Third-party software\n", "Full dependency sources and their original notices/license texts are in source/vendor/.",
               "This includes cross-platform, build and example-client dependencies; not every package is linked into each executable.",
               "LAME 3.100 and mp3lame-sys 0.1.11 are statically linked. See source/docs/mp3-encoder.md and source/docs/release.md for relinking.",
               "Inter is bundled under its OFL notice in source/assets/fonts/. Rust library source/notices are in source/toolchain-rust-library/.",
               "\n| Package | Version | Declared license | Source |", "| --- | --- | --- | --- |"]
    for package in packages:
        notices.append(f"| {package['name']} | {package['version']} | {package['license']} | {package['source']} |")
    runtime_notices = temporary / "rust-notices"
    runtime_notices.mkdir()
    for filename in ["LICENSE-APACHE", "LICENSE-MIT", "COPYRIGHT"]:
        url = f"https://raw.githubusercontent.com/rust-lang/rust/{rust_fields['commit-hash']}/{filename}"
        with urllib.request.urlopen(url, timeout=60) as response:
            (runtime_notices / filename).write_bytes(response.read())
    files = run("git", "ls-files", "-z", capture=True).split("\0")
    assert run("git", "rev-parse", "HEAD", capture=True).strip() == commit, "Commit changed during packaging"
    dirty = bool(run("git", "status", "--porcelain", capture=True).strip())
    assert not dirty, "Source changed during packaging"
    binaries = {binary: ROOT / "target/release" / (binary + suffix)
                for binary in ["wavebob", "wavebob-headless", "wavebob-mcp"]}
    binaries["wavebob-workflow"] = ROOT / "target/release/examples" / ("workflow" + suffix)
    manifest = {"version": version, "host": host, "platform": platform.platform(), "commit": commit, "working_tree_dirty": dirty,
                "created_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "rustc": rust,
                "cargo_lock_sha256": digest(ROOT / "Cargo.lock"), "dependencies": packages,
                "binaries": {binary + suffix: {"sha256": digest(path), "bytes": path.stat().st_size}
                             for binary, path in binaries.items()},
                "automation_features": [], "gui_features": ["gui"], "signing": "unsigned development kit"}
    configuration = '[source.crates-io]\nreplace-with = "vendored-sources"\n[source.vendored-sources]\ndirectory = "vendor"\n'
    start = """# WaveBob local development kit

Launch bin/wavebob for the editor. The bin directory also contains the GUI-free
wavebob-headless and wavebob-mcp runners and the wavebob-workflow example client.
On Windows use the .exe files. See source/docs/release.md for runtime prerequisites,
source/docs/commands.md for discovery/attachment, source/docs/mcp.md for MCP/PNG
retrieval and source/docs/automation.md for the reproducible editing example.

Keep this kit's source, dependency sources and notices with the executables.
It includes full application and encoder source and offline rebuilding/relinking
instructions. Application licensing for public redistribution is not selected;
this kit is for the user's local development/review. Third-party licenses remain
in their original sources. Signing, installers and automatic updates are deferred.
"""
    # ZipInfo preserves executable mode on Unix; ZipFile uses Zip64 when needed.
    staged_archive = temporary / archive.name
    with zipfile.ZipFile(staged_archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as output:
        for binary, path in binaries.items():
            output.write(path, f"{name}/bin/{binary}{suffix}")
        for relative in sorted(filter(None, files)):
            path = ROOT / relative
            if path.is_file() and relative != "rust-toolchain.toml":
                output.write(path, f"{name}/source/{relative}")
        for directory, destination in [(vendor, "source/vendor"), (rust_source, "source/toolchain-rust-library"), (runtime_notices, "source/toolchain-rust-library")]:
            for path in sorted(directory.rglob("*")):
                if path.is_file():
                    output.write(path, f"{name}/{destination}/{path.relative_to(directory).as_posix()}")
        output.writestr(f"{name}/source/.cargo/config.toml", configuration)
        output.writestr(f"{name}/source/rust-toolchain.toml", f'[toolchain]\nchannel = "{rust_fields["release"]}"\nprofile = "minimal"\ncomponents = ["rustfmt", "clippy", "rust-src"]\n')
        output.writestr(f"{name}/START-HERE.md", start)
        output.writestr(f"{name}/THIRD_PARTY_NOTICES.md", "\n\n".join(notices) + "\n")
        output.writestr(f"{name}/BUILD-MANIFEST.json", json.dumps(manifest, indent=2) + "\n")
    checksum = digest(staged_archive)
    os.link(staged_archive, archive)  # Atomic, exclusive publication on the same volume.
    (archive.with_suffix(".zip.sha256")).write_text(f"{checksum}  {archive.name}\n", encoding="utf-8")
    print(json.dumps({"archive": str(archive), "sha256": checksum, "bytes": archive.stat().st_size,
                      "commit": commit, "dirty": dirty, "dependency_sources": len(packages)}, indent=2))
