"""Capture original pinned ordinary EQ PCM and frequency responses."""
import argparse, gzip, hashlib, io, json, math, struct, subprocess, sys
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference_root',type=Path)
parser.add_argument('--rustc',default='rustc')
args=parser.parse_args()
repo=Path(__file__).resolve().parents[2]
source=args.reference_root.resolve()
work=repo/'.tools/reference-parametric-eq';work.mkdir(parents=True,exist_ok=True)
output=repo/'tests/fixtures/processing/parametric-eq';output.mkdir(parents=True,exist_ok=True)
pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text(encoding='utf-8'))
sha=lambda raw:hashlib.sha256(raw).hexdigest()
sources={}
for relative,expected in pin['sha256'].items():
    if relative.startswith('crates/os-engine/src/effects/eq'):
        actual=sha((source/relative).read_bytes());assert actual==expected,relative;sources[relative]=actual
reference_modules=work/'engine/eq';reference_modules.mkdir(parents=True,exist_ok=True)
for relative in sources:
    destination=reference_modules/('mod.rs' if relative.endswith('/eq.rs') else Path(relative).name)
    destination.write_bytes((source/relative).read_bytes())
main=work/'main.rs';main.write_text(Path(__file__).with_name('parametric_eq.rs.template').read_text().replace('@WORK@',work.as_posix()),encoding='utf-8')
binary=work/('reference.exe' if sys.platform=='win32' else 'reference')
subprocess.run([args.rustc,'--edition=2024','-O',str(main),'-o',str(binary)],check=True)
defaults=[0]*7+[10923,18204,25486,32768,40050,47332,54613]+[40124]+[25412]*5+[40124]+[5]+[6]*5+[7]+[0]*8
assert len(defaults)==36
cases=[]
def case(name, changes=None, rate=48000,mask=127,stimulus='impulse',frames=16384,role='development'):
    params=defaults.copy()
    for index,value in (changes or {}).items():params[index]=value
    cases.append(dict(name=name,parameters=params,sample_rate=rate,mask=mask,stimulus=stimulus,frames=frames,role=role))
for rate in [44100,48000,96000]:
    case(f'default-{rate}',rate=rate,stimulus='burst')
    case(f'peak-{rate}',{3:1200,10:37115},rate=rate,stimulus='burst',frames=32768)
for kind in range(1,8):
    for order in range(-3,4):
        case(f'shape-{kind}-order-{order}',{3:1200,24:kind,31:order},stimulus='burst')
    for freq,width,gain in [(0,0,1800),(65536,65536,-1800),(0,65536,-1800),(65536,0,1800)]:
        case(f'extreme-{kind}-{freq}-{width}',{3:gain,10:freq,17:width,24:kind,31:-3})
case('all-disabled',{35:600},mask=0,stimulus='burst')
case('disabled-types',{21+k:0 for k in range(7)},stimulus='burst')
case('main-cut',{35:-1800},stimulus='burst')
case('zero-input',{3:1800},stimulus='silence')
case('gate-and-quiet', {3:1800,10:0,17:0}, frames=48000)
case('held-out-mixed-96000',{0:-317,1:671,2:-993,3:1437,4:-1122,5:752,6:-128,7:7391,8:16442,9:24415,10:33371,11:41107,12:48813,13:61003,14:17811,15:63211,16:7111,17:49121,18:21391,19:37777,20:41113,21:5,22:6,23:4,24:6,25:2,26:6,27:7,28:-2,29:1,30:0,31:-1,32:3,33:-3,34:2,35:117},rate=96000,mask=123,stimulus='burst',frames=65536,role='held-out')
def store(path,raw):
    buf=io.BytesIO()
    with gzip.GzipFile(filename='',mode='wb',fileobj=buf,mtime=0) as out:out.write(raw)
    packed=buf.getvalue();(output/path).write_bytes(packed)
    return sha(raw),sha(packed)
for c in cases:
    values=[];seed=3219807;rate=c['sample_rate']
    for frame in range(c['frames']):
        if c['stimulus']=='impulse':left,right=(1.0,-.375) if frame==0 else (0.0,0.0)
        elif c['stimulus']=='silence':left=right=0.0
        elif frame<min(8192,rate//8):
            seed=(1664525*seed+1013904223)&0xffffffff;noise=((seed>>8)/16777216-.5)*.03
            left=.17*math.sin(math.tau*131*frame/rate)+.11*math.sin(math.tau*1733*frame/rate)+noise
            right=.13*math.cos(math.tau*349*frame/rate)-.09*math.sin(math.tau*7993*frame/rate)-noise
        else:left=right=0.0
        values.extend([left,right])
    raw=struct.pack('<'+'f'*len(values),*values)
    input_path=work/'input.f32';output_path=work/'output.f32';input_path.write_bytes(raw)
    result=subprocess.run([str(binary),str(input_path),str(output_path),str(rate),str(c['mask']),','.join(map(str,c['parameters']))],check=True,capture_output=True,text=True)
    expected=output_path.read_bytes();samples=struct.unpack('<'+'f'*(len(expected)//4),expected)
    assert all(math.isfinite(v) for v in samples),c['name']
    c['response']=[list(map(float,line.split(','))) for line in result.stdout.splitlines()]
    assert all(math.isfinite(v) for row in c['response'] for v in row),c['name']
    for key,bytes_ in [('input',raw),('output',expected)]:
        c[key]=c['name']+'-'+key+'.f32.gz';c[key+'_sha256'],c[key+'_gzip_sha256']=store(c[key],bytes_)
    c.update(block_frames=64,reset=True,preroll_frames=8192,peak=max(map(abs,samples)),channel_energy=[sum(v*v for v in samples[ch::2]) for ch in range(2)])
manifest=dict(repository=pin['repository'],commit=pin['commit'],sources=sources,compiler=subprocess.check_output([args.rustc,'--version'],text=True).strip(),reference_binary_sha256=sha(binary.read_bytes()),maximum_absolute_error=2e-6,maximum_relative_rms_error=2e-5,energy_floor=1e-12,response_db_tolerance=2e-5,alignment_frames=0,gain_correction=1.0,cases=cases)
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(f'Generated {len(cases)} ordinary EQ reference captures')
