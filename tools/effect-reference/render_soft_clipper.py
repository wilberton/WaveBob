"""Reproduce Soft Clipper fixtures from unchanged, pinned upstream Rust sources.

Usage: python render_soft_clipper.py REFERENCE_SOURCE_ROOT [--rustc PATH]
Source tree must match pinned commit 0af860dfd31c7ee9cbf8af6747617e031a1a7742.
No production WaveBob code is compiled into the reference executable.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference_root', type=Path)
parser.add_argument('--rustc', default='rustc')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
source = args.reference_root.resolve()
work = repo / '.tools' / 'reference-soft-clipper'
work.mkdir(parents=True, exist_ok=True)
output = repo / 'tests' / 'fixtures' / 'processing' / 'soft-clipper'
output.mkdir(parents=True, exist_ok=True)
paths = ['crates/os-engine/src/effects/soft_clipper.rs', 'crates/os-dsp/src/level.rs']
manifest_path = repo / '.scratch' / 'wavebob' / 'reference-manifest.json'
pinned = json.loads(manifest_path.read_text(encoding='utf-8'))
sha = lambda data: hashlib.sha256(data).hexdigest()
for path in paths:
    actual = sha((source / path).read_bytes())
    if actual != pinned['sha256'][path]:
        sys.exit(f'Pinned source checksum mismatch: {path}')
main = work / 'main.rs'
main.write_text((Path(__file__).with_name('soft_clipper.rs.template')).read_text(encoding='utf-8-sig').replace('@ROOT@', source.as_posix()), encoding='utf-8')
binary = work / ('reference.exe' if sys.platform == 'win32' else 'reference')
subprocess.run([args.rustc, '--edition=2024', '-O', str(main), '-o', str(binary)], check=True)
compiler = subprocess.check_output([args.rustc, '--version'], text=True).strip()
cases = []
for rate in [44100, 48000, 96000]:
    samples = []
    # Shared stereo fixture: silence, impulses, steps, dense bipolar sweep,
    # exact threshold neighbors, independent tones and finite overloads.
    samples.extend([0.0, -0.0] * 64)
    samples.extend([1.0, -1.0] + [0.0, 0.0] * 63)
    for value in [-4.0, -1.0, -0.5, 0.0, 0.5, 1.0, 4.0]:
        samples.extend([value, -value] * 32)
    for i in range(4097):
        value = -4.0 + 8.0 * i / 4096
        samples.extend([value, -value * 0.731])
    for threshold in [1, 32, 64, 90, 100, 127]:
        level = threshold / 128
        for value in [level - 2**-24, level, level + 2**-24]:
            samples.extend([value, -value])
    for frame in range(2048):
        samples.extend([1.2 * math.sin(2 * math.pi * 997 * frame / rate) + 0.4 * math.sin(2 * math.pi * 4999 * frame / rate), 1.5 * math.sin(2 * math.pi * 1733 * frame / rate)])
    samples.extend([3.4028234663852886e38, -3.4028234663852886e38, 1e-30, -1e-30])
    pcm = struct.pack('<' + 'f' * len(samples), *samples)
    input_path = output / f'input-{rate}.f32'
    input_path.write_bytes(pcm)
    for threshold in [1, 100, 127]:
        for post in [0, 64, 128, 160]:
            name = f'{rate}-t{threshold}-p{post}.f32'
            result = output / name
            subprocess.run([str(binary), str(input_path), str(result), str(threshold), str(post)], check=True)
            cases.append({'sample_rate': rate, 'channels': 2, 'threshold': threshold, 'post': post, 'input': input_path.name, 'input_sha256': sha(pcm), 'output': name, 'output_sha256': sha(result.read_bytes()), 'reset': True, 'seed': None, 'tempo': 120.0, 'block_policy': 'memoryless sample path; WaveBob tests use 1/7/64-frame blocks', 'frames': len(samples)//2})
manifest = {'repository': pinned['repository'], 'commit': pinned['commit'], 'compiler': compiler, 'sources': {path:pinned['sha256'][path] for path in paths}, 'maximum_absolute_error': 2e-7, 'alignment_frames':0, 'gain_correction':1.0, 'cases':cases}
(output / 'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(f'Generated {len(cases)} pinned reference cases in {output}')
