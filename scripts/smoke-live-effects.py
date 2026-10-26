"""Exercise all built-in processors and portable bounces in an extracted release.

Uses only the shipped NDJSON runner and Python's standard library. Output must be
new; the one source file deleted for portability is created inside that directory.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import time
import wave


class Client:
    def __init__(self, binary):
        self.child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                     text=True, encoding="utf-8",
                                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.sequence = 0

    def call(self, command, params):
        self.sequence += 1
        request = {"id": str(self.sequence), "command": command, "params": params}
        self.child.stdin.write(json.dumps(request) + "\n")
        self.child.stdin.flush()
        line = self.child.stdout.readline()
        assert line, f"Runner closed during {command}"
        result = json.loads(line)
        assert result["status"] == "ok", result
        return result["result"]

    def job(self, command, params):
        started = self.call(command, params)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            job = self.call("jobs.get", {"job_id": started["id"]})
            if job["status"] != "running":
                assert job["status"] == "succeeded", job
                return job
            time.sleep(0.002)
        raise AssertionError(f"Timed out: {command}")

    def edit(self, owner, change):
        state = self.call("processing.get", {"owner": owner})
        return self.call("processing.edit", {"owner": owner,
                         "expected_edit_revision": state["committed"]["edit_revision_id"],
                         "change": change})

    def prepare(self):
        arrangement = self.call("arrangement.get", {})
        self.job("mix.prepare", {"revision_id": arrangement["revision_id"]})
        return self.call("arrangement.get", {})

    def samples(self):
        arrangement = self.call("arrangement.get", {})
        return self.call("mix.samples", {"revision_id": arrangement["revision_id"],
                         "start_frame": 0, "frames": 4096})["channels"]

    def close(self):
        self.child.stdin.close()
        try:
            code = self.child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait()
            raise
        errors = self.child.stderr.read()
        assert code == 0, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = args.out.resolve()
    assert not root.exists(), "Use a new output directory"
    root.mkdir(parents=True)
    source = root / "Owned source.wav"
    with wave.open(str(source), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(48000)
        output.writeframes(b"".join(struct.pack("<h", round(math.sin(i * 0.071) * 8192))
                                   for i in range(16384)))
    client = Client(binary)
    document = client.job("documents.open", {"path": str(source)})["result"]
    owner = {"kind": "document", "id": document["id"]}
    original = client.call("documents.samples", {"document_id": document["id"], "frames": 4096})["channels"]
    catalogue = client.call("processing.catalogue", {})
    assert len(catalogue) == 11 and all(row["available"] for row in catalogue)
    renders = []
    for row in catalogue:
        state = client.edit(owner, {"action": "add", "kind": row["kind"]})
        effect = state["committed"]["effects"][0]["id"]
        render = client.job("processing.render", {"owner": owner})["output"]["details"]
        count = min(4096, render["source_body_frames"] + render["tail_frames"])
        values = client.call("processing.samples", {"audio_revision_id": render["audio_revision_id"],
                             "start_frame": 0, "frames": count})["channels"]
        assert all(math.isfinite(value) for channel in values for value in channel)
        assert render["render_identity"] == state["render_identity"]
        renders.append({"kind": row["kind"], "metadata": render})
        client.edit(owner, {"action": "remove", "effect_id": effect})
    client.edit(owner, {"action": "add", "kind": "balance"})
    arrangement = client.call("tracks.add", {})
    track = arrangement["tracks"][0]["id"]
    for start, end, position in [(0, 16384, 0), (0, 16384, 20000), (1000, 4000, 50000)]:
        client.call("clips.insert", {"track_id": track, "document_id": document["id"],
                    "revision_id": document["revision_id"], "source_start": start,
                    "source_end": end, "position": position})
    arrangement = client.prepare()
    clips = arrangement["tracks"][0]["clips"]
    original_bounce = clips[0]["bounce"]["audio_revision_id"]
    assert original_bounce == clips[1]["bounce"]["audio_revision_id"]
    assert original_bounce != clips[2]["bounce"]["audio_revision_id"]
    initial_cache = client.call("mix.get", {})
    assert initial_cache["bounce_cache_entries"] == 2
    client.edit(owner, {"action": "set_envelope", "envelope": {
                "gain_db": -3.0, "fade_in_frames": 0, "fade_out_frames": 0}})
    client.prepare()
    changed_cache = client.call("mix.get", {})
    client.call("history.undo", {})
    restored = client.prepare()
    assert restored["tracks"][0]["clips"][0]["bounce"]["audio_revision_id"] == original_bounce
    restored_cache = client.call("mix.get", {})
    assert changed_cache["bounce_cache_entries"] == restored_cache["bounce_cache_entries"]
    assert original == client.call("documents.samples", {"document_id": document["id"],
                                  "frames": 4096})["channels"]
    before = client.samples()
    project = root / "Portable.wavebob"
    saved = client.job("project.save", {"path": str(project)})["output"]["details"]
    outputs = []
    for name, encoding in [("Rendered.wav", {"kind": "wav", "encoding": "float32"}),
                           ("Rendered.mp3", {"kind": "mp3", "bitrate_kbps": 192})]:
        detail = client.job("mix.export", {"revision_id": restored["revision_id"],
                            "start_frame": 0, "end_frame": 53000, "include_tail": True,
                            "path": str(root / name), "format": encoding})["output"]["details"]
        outputs.append(detail)
    decoded = client.job("documents.open", {"path": str(root / "Rendered.mp3")})["result"]
    assert decoded["frames"] == outputs[1]["frames"] and decoded["channels"] == 2
    client.close()
    assert source.resolve().is_relative_to(root)
    source.unlink()
    client = Client(binary)
    try:
        opened = client.job("project.open", {"path": str(project)})["output"]["details"]
        client.prepare()
        assert before == client.samples()
    finally:
        client.close()
    record = {"binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "effects_rendered": renders, "cache_initial": initial_cache,
              "cache_changed": changed_cache, "cache_restored": restored_cache,
              "shared_full_and_distinct_trim": True, "undo_reused_asset": True,
              "source_pcm_unchanged": True, "fresh_session_after_source_deletion_matches": True,
              "saved": saved, "opened": opened, "outputs": outputs,
              "mp3_reimport_frames": decoded["frames"], "physical_listening": "unverified"}
    (root / "release-smoke.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"effects": len(renders), "cache_initial_bytes": initial_cache["bounce_cache_bytes"],
                      "cache_after_undo_bytes": restored_cache["bounce_cache_bytes"],
                      "portable_assets": saved["assets"], "status": "passed"}))


if __name__ == "__main__":
    main()
