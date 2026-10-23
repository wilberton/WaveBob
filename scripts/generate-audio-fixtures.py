"""Generate owned MP3 test signals and independent FFmpeg PCM references."""
import argparse
import hashlib
import json
import math
import pathlib
import struct
import subprocess
import tempfile
import wave

parser = argparse.ArgumentParser()
parser.add_argument("--ffmpeg", required=True)
args = parser.parse_args()
output = pathlib.Path(__file__).resolve().parents[1] / "tests/fixtures/audio"
output.mkdir(parents=True, exist_ok=True)
version = subprocess.check_output([args.ffmpeg, "-version"], text=True).splitlines()[0]
records = []
with tempfile.TemporaryDirectory() as temp:
    for channels, rate, mode, tags in [(1,44100,"cbr",True),(2,48000,"cbr",True),(1,22050,"vbr",True),(2,44100,"vbr",True),(1,44100,"cbr",False),(2,48000,"vbr",False)]:
        name = f"{mode}-{channels}ch-{rate}-{'tagged' if tags else 'untagged'}"
        source = pathlib.Path(temp) / "source.wav"
        frames = 8192
        with wave.open(str(source), "wb") as writer:
            writer.setnchannels(channels)
            writer.setsampwidth(2)
            writer.setframerate(rate)
            values = []
            for frame in range(frames):
                for channel in range(channels):
                    t = frame / rate
                    # Intentional silence at each end must survive metadata-free import.
                    value = 0 if frame < 500 or frame >= frames - 500 else int(12000 * math.sin(2 * math.pi * (440 + channel * 173) * t) * (0.5 + 0.5 * math.sin(2 * math.pi * 17 * t)**2))
                    values.append(value)
            writer.writeframes(struct.pack("<" + "h" * len(values), *values))
        mp3 = output / f"{name}.mp3"
        encoding = ["-b:a", "128k"] if mode == "cbr" else ["-q:a", "2"]
        subprocess.run([args.ffmpeg,"-y","-v","error","-i",str(source),"-codec:a","libmp3lame",*encoding,"-write_xing","1" if tags else "0","-id3v2_version","3" if tags else "0","-map_metadata","-1",str(mp3)],check=True)
        reference = output / f"{name}.f32"
        subprocess.run([args.ffmpeg,"-y","-v","error","-i",str(mp3),"-f","f32le","-codec:a","pcm_f32le",str(reference)],check=True)
        records.append(dict(name=name,channels=channels,sample_rate=rate,mode=mode,tags=tags,source_frames=frames,decoded_frames=reference.stat().st_size//(4*channels),mp3_sha256=hashlib.sha256(mp3.read_bytes()).hexdigest(),reference_sha256=hashlib.sha256(reference.read_bytes()).hexdigest()))
(output/"manifest.json").write_text(json.dumps(dict(generator=version,fixtures=records),indent=2)+"\n",encoding="utf-8")
print(json.dumps(records,indent=2))
