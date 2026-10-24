"""Independent QA decoder; FFmpeg is never a WaveBob runtime dependency."""
import argparse
import array
import json
import math
import pathlib
import re
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--ffmpeg", required=True)
parser.add_argument("directory", type=pathlib.Path)
args = parser.parse_args()


def decode(name):
    result = subprocess.run([args.ffmpeg, "-hide_banner", "-i", str(args.directory / name),
                             "-f", "f32le", "-acodec", "pcm_f32le", "-"],
                            capture_output=True, check=True)
    match = re.search(r"Audio:.*?, (\d+) Hz, (stereo|mono)", result.stderr.decode())
    assert match, result.stderr.decode()
    rate = int(match[1])
    channels = 2 if match[2] == "stereo" else 1
    pcm = array.array("f", result.stdout)
    if sys.byteorder != "little":
        pcm.byteswap()
    return pcm, rate, channels


reference, rate, channels = decode("Mix.wav")
actual, decoded_rate, decoded_channels = decode("Mix.mp3")
assert (rate, channels, len(reference)) == (48000, 2, 576000 * 2)
assert (decoded_rate, decoded_channels, len(actual)) == (rate, channels, len(reference))
edge = 1024 * channels
pairs = list(zip(reference[edge:-edge], actual[edge:-edge]))
rmse = math.sqrt(sum((a - b) ** 2 for a, b in pairs) / len(pairs))
maximum = max(abs(a - b) for a, b in pairs)
correlation = sum(a * b for a, b in pairs) / math.sqrt(sum(a * a for a, b in pairs) * sum(b * b for a, b in pairs))
assert rmse < 0.03 and maximum < 0.12 and correlation > 0.98, (rmse, maximum, correlation)
sample, sample_rate, sample_channels = decode("Sample selection.mp3")
assert (sample_rate, sample_channels, len(sample)) == (44100, 2, 176400 * 2)
record = {"decoder": subprocess.check_output([args.ffmpeg, "-version"], text=True).splitlines()[0],
          "mix": {"frames": len(actual) // channels, "rate": rate, "channels": channels,
                  "rmse": rmse, "max_error": maximum, "correlation": correlation,
                  "excluded_edge_frames": 1024, "alignment_offset_frames": 0},
          "sample": {"frames": len(sample) // sample_channels, "rate": sample_rate, "channels": sample_channels}}
(args.directory / "independent-mp3.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(record, indent=2))
