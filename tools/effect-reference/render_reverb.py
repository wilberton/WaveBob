"""Render pinned, unchanged reference Reverb into lossless f32/gzip fixtures."""
import argparse
import gzip
import hashlib
import io
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
work = repo / '.tools/reference-reverb'
work.mkdir(parents=True, exist_ok=True)
output = repo / 'tests/fixtures/processing/reverb'
output.mkdir(parents=True, exist_ok=True)
pin = json.loads((repo / '.scratch/wavebob/reference-manifest.json').read_text(encoding='utf-8'))
relative = 'crates/os-engine/src/effects/reverb.rs'
sha = lambda raw: hashlib.sha256(raw).hexdigest()
assert sha((source / relative).read_bytes()) == pin['sha256'][relative]
main = work / 'main.rs'
main.write_text(Path(__file__).with_name('reverb.rs.template').read_text(encoding='utf-8').replace('@ROOT@', source.as_posix()), encoding='utf-8')
binary = work / ('reference.exe' if sys.platform == 'win32' else 'reference')
subprocess.run([args.rustc, '--edition=2024', '-O', str(main), '-o', str(binary)], check=True)
defaults = [75,40,0,50,100,15,40,100,500,0,128,128,128,684,0]
ids = ['low_cut','high_cut','predelay','size','diffusion','decay','damping','bass','crossover','separation','dry','early','wet','mod_speed','mod_depth']
cases = []
def case(name, changes=None, *, rate=48000, bpm=120, sync=False, side=False, stimulus='left', seconds=3, role='development'):
    knobs = defaults.copy()
    for key,value in (changes or {}).items():
        knobs[ids.index(key)] = value
    cases.append(dict(name=name,knobs=knobs,sample_rate=rate,tempo=bpm,tempo_sync=sync,mid_side=side,stimulus=stimulus,frames=rate*seconds,role=role))
for rate in [44100,48000,96000]:
    case(f'default-{rate}',rate=rate)
case('right-impulse',stimulus='right')
case('dry-only',{'early':0,'wet':0},stimulus='burst')
case('early-only',{'dry':0,'wet':0})
case('wet-only',{'dry':0,'early':0})
case('side-filter-off',{'dry':0,'low_cut':19,'high_cut':221,'damping':221},side=True,stimulus='burst')
case('side-cancellation',{'dry':0},side=True,stimulus='correlated')
case('cuts-extreme',{'low_cut':3000,'high_cut':5,'damping':5},stimulus='burst')
case('predelay-free',{'predelay':384,'dry':0,'early':0})
case('predelay-sync',{'predelay':192,'dry':0,'early':0},sync=True,bpm=90)
case('predelay-six-seconds',{'predelay':384,'dry':0,'early':0},sync=True,bpm=20,seconds=8)
case('small-room',{'size':1,'dry':0})
case('large-room',{'size':100,'dry':0})
case('no-diffusion',{'diffusion':0,'size':65},stimulus='burst')
case('half-diffusion',{'diffusion':50},stimulus='burst')
case('modulation-fast',{'mod_speed':2048,'mod_depth':2048},rate=44100,seconds=8,stimulus='burst')
case('modulation-slow',{'mod_speed':0,'mod_depth':2048},seconds=8,stimulus='burst')
case('bass-long-decay',{'bass':300,'decay':200,'size':100,'low_cut':19,'high_cut':221,'damping':221,'dry':0},seconds=8,stimulus='burst')
case('separated',{'separation':-64,'dry':0},stimulus='burst')
case('merged',{'separation':64,'dry':0},stimulus='burst')
case('maximum-levels',{'dry':160,'early':160,'wet':160},stimulus='burst')
case('silence',{'mod_depth':2048},stimulus='silence')
case('held-out-96k',{'size':73,'diffusion':61,'mod_speed':1777,'mod_depth':733,'bass':172,'crossover':777},rate=96000,stimulus='burst',seconds=5,role='held-out')
def compressed(raw):
    data = io.BytesIO()
    with gzip.GzipFile(fileobj=data, mode='wb', filename='', mtime=0, compresslevel=9) as file:
        file.write(raw)
    return data.getvalue()
for item in cases:
    rate,frames = item['sample_rate'],item['frames']
    values = []
    seed = 3107401
    for frame in range(frames):
        left=right=0.0
        if item['stimulus']=='left' and frame==0: left=1.0
        if item['stimulus']=='right' and frame==0: right=1.0
        if item['stimulus']=='correlated' and frame<rate//8:
            left=right=0.4*math.sin(math.tau*123*frame/rate)
        if item['stimulus']=='burst' and frame<rate//4:
            seed=(seed*1664525+1013904223)&0xffffffff
            noise=(seed/4294967296.0*2-1)*0.07
            left=0.31*math.sin(math.tau*123*frame/rate)+0.19*math.sin(math.tau*997*frame/rate)+noise
            right=0.27*math.sin(math.tau*73*frame/rate)+0.17*math.sin(math.tau*1733*frame/rate)-noise
        values.extend([left,right])
    raw=struct.pack('<'+'f'*len(values),*values)
    input_path=work/(item['name']+'-input.f32')
    output_path=work/(item['name']+'-output.f32')
    input_path.write_bytes(raw)
    subprocess.run([str(binary),str(input_path),str(output_path),str(rate),str(item['tempo']),'1' if item['tempo_sync'] else '0','1' if item['mid_side'] else '0',','.join(map(str,item['knobs']))],check=True)
    rendered=output_path.read_bytes()
    samples=struct.unpack('<'+'f'*(len(rendered)//4),rendered)
    assert all(math.isfinite(value) for value in samples)
    for kind,data in [('input',raw),('output',rendered)]:
        name=item['name']+'-'+kind+'.f32.gz'
        packed=compressed(data)
        (output/name).write_bytes(packed)
        item[kind]=name
        item[kind+'_sha256']=sha(data)
        item[kind+'_gzip_sha256']=sha(packed)
    item['reference_peak']=max(map(abs,samples))
    item['channel_energy']=[sum(value*value for value in samples[channel::2]) for channel in [0,1]]
    item['block_frames']=64
    item['reset']=True
    item['phase']=0
    print(item['name'],flush=True)
manifest={'repository':pin['repository'],'commit':pin['commit'],'sources':{relative:pin['sha256'][relative]},'compiler':subprocess.check_output([args.rustc,'--version'],text=True).strip(),'reference_binary_sha256':sha(binary.read_bytes()),'maximum_absolute_error':2e-6,'maximum_relative_rms_error':2e-5,'energy_floor':1e-12,'alignment_frames':0,'gain_correction':1.0,'cases':cases}
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(f'Generated {len(cases)} complete lossless reference cases')
