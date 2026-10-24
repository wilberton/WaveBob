"""Verify an extracted native kit or a fresh ZIP using its shipped executables."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
import time
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument("archive", type=pathlib.Path)
parser.add_argument("--out", required=True, type=pathlib.Path)
parser.add_argument("--gui", action="store_true")
parser.add_argument("--audition", action="store_true")
args = parser.parse_args()
assert not args.out.exists(), "Use a fresh smoke-test directory"
args.out.mkdir(parents=True)
args.out = args.out.resolve()
creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


archive_hash = digest(args.archive)
sidecar = args.archive.with_suffix(".zip.sha256")
assert sidecar.read_text().split()[0] == archive_hash
with zipfile.ZipFile(args.archive) as archive:
    for entry in archive.infolist():
        target = (args.out / entry.filename).resolve()
        assert target.is_relative_to(args.out), f"Unsafe archive path: {entry.filename}"
    archive.extractall(args.out)
kit = next(path for path in args.out.iterdir() if path.is_dir())
manifest = json.loads((kit / "BUILD-MANIFEST.json").read_text())
suffix = ".exe" if "windows" in manifest["host"] else ""
for name, info in manifest["binaries"].items():
    path = kit / "bin" / name
    if suffix == "":
        path.chmod(0o755)
    assert digest(path) == info["sha256"], name


def executable(name):
    return str(kit / "bin" / (name + suffix))


def command(*items, **kwargs):
    result = subprocess.run(items, capture_output=True, text=True,
                            creationflags=creationflags, timeout=240, **kwargs)
    if result.returncode:
        print(result.stderr[-16000:], file=sys.stderr)
        if result.stdout:
            print(result.stdout[-16000:], file=sys.stderr)
    result.check_returncode()
    return result


results = {}
for adapter, binary in [("direct", None), ("headless", "wavebob-headless"), ("mcp", "wavebob-mcp")]:
    print(f"Shipped {adapter} workflow", flush=True)
    items = [executable("wavebob-workflow"), "--adapter", adapter, "--out", str(args.out / adapter)]
    if binary:
        items += ["--binary", executable(binary)]
    result = command(*items)
    results[adapter] = json.loads(result.stdout)
    assert results[adapter]["project_reopen_without_sources"] is True
output_hashes = {}
for name in ["Mix.wav", "Mix.mp3", "Sample selection.mp3"]:
    hashes = {adapter: digest(args.out / adapter / name) for adapter in results}
    assert len(set(hashes.values())) == 1, hashes
    output_hashes[name] = hashes["direct"]
results["output_hashes"] = output_hashes

if args.gui:
    print("Shipped GUI attachment and MCP screenshots", flush=True)
    runtime = args.out / "runtime"
    runtime.mkdir()
    native_log = (args.out / "gui-native.log").open("wb")
    gui = subprocess.Popen([executable("wavebob"), "--hidden", "--name", "Release smoke",
                            "--runtime-dir", str(runtime), "--preferences", str(args.out / "layout.json")],
                           stdout=subprocess.DEVNULL, stderr=native_log, creationflags=creationflags)
    try:
        deadline = time.monotonic() + 30
        while True:
            listing = json.loads(command(executable("wavebob-mcp"), "--list-instances", "--runtime-dir", str(runtime)).stdout)
            if listing:
                session = listing[0]["session_id"]
                break
            assert gui.poll() is None, "GUI exited before registration"
            assert time.monotonic() < deadline
            time.sleep(0.05)
        items = [executable("wavebob-workflow"), "--adapter", "mcp", "--binary", executable("wavebob-mcp"),
                 "--attach", session, "--runtime-dir", str(runtime), "--out", str(args.out / "gui")]
        if args.audition:
            items.append("--audition")
        results["gui"] = json.loads(command(*items).stdout)
        request = json.dumps({"id": "smoke-quit", "command": "presentation.quit", "params": {}}) + "\n"
        closed = json.loads(command(executable("wavebob-headless"), "--attach", session,
                                    "--runtime-dir", str(runtime), input=request).stdout)
        assert closed["status"] == "ok", closed
        assert gui.wait(timeout=20) == 0
    finally:
        if gui.poll() is None:
            gui.kill()
            gui.wait()
        native_log.close()
        if gui.returncode:
            print((args.out / "gui-native.log").read_text(encoding="utf-8", errors="replace")[-16000:])
record = {"archive": str(args.archive.resolve()), "archive_sha256": archive_hash,
          "build_commit": manifest["commit"], "host": manifest["host"], "results": results}
(args.out / "smoke-result.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"host": manifest["host"], "gui_checked": args.gui,
                  "result": str(args.out / "smoke-result.json")}, indent=2))
