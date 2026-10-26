"""Render original pinned Fast Distortion tables and stereo PCM."""
import argparse,gzip,hashlib,io,json,math,struct,subprocess,sys
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('reference_root',type=Path);parser.add_argument('--rustc',default='rustc');args=parser.parse_args()
repo=Path(__file__).resolve().parents[2];source=args.reference_root.resolve();work=repo/'.tools/reference-fast-distortion';work.mkdir(parents=True,exist_ok=True);output=repo/'tests/fixtures/processing/fast-distortion';output.mkdir(parents=True,exist_ok=True)
pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text(encoding='utf-8'));sha=lambda raw:hashlib.sha256(raw).hexdigest()
sources={}
for relative in ['crates/os-engine/src/effects/fast_dist.rs','crates/os-dsp/src/level.rs']:
    sources[relative]=sha((source/relative).read_bytes());assert sources[relative]==pin['sha256'][relative]
main=work/'main.rs';main.write_text(Path(__file__).with_name('fast_distortion.rs.template').read_text().replace('@ROOT@',source.as_posix()),encoding='utf-8');binary=work/('reference.exe' if sys.platform=='win32' else 'reference')
subprocess.run([args.rustc,'--edition=2024','-O',str(main),'-o',str(binary)],check=True)
def pack(values):return struct.pack('<'+'f'*len(values),*values)
def store(name,raw):
    out=io.BytesIO()
    with gzip.GzipFile(fileobj=out,filename='',mode='wb',mtime=0) as f:f.write(raw)
    packed=out.getvalue();(output/name).write_bytes(packed);return sha(raw),sha(packed)
inputs={}
for rate in [44100,48000,96000]:
    values=[]
    for i in range(8192):values.extend([i/8191,-i/8191])
    for i in range(8191):values.append((i+.5)/8191)
    for i in [0,1,31,100,1024,4095,4096,8190,8191]:
        v=struct.unpack('<I',struct.pack('<f',i/8191))[0]
        for bits in [max(0,v-1),v,v+1]:
            x=struct.unpack('<f',struct.pack('<I',bits))[0];values.extend([x,-x])
    values.extend([0.0,-0.0,1e-30,-1e-30,.03125,-.03125,2,-2,10000,-10000,1000000,-1000000])
    values.extend([.125]*64+[-.125]*64)
    frames=[]
    for i,value in enumerate(values):frames.extend([value,value*-.731])
    for i in range(4096):frames.extend([.65*math.sin(math.tau*997*i/rate)+.2*math.sin(math.tau*11371*i/rate),.7*math.sin(math.tau*1733*i/rate)-.11*math.cos(math.tau*131*i/rate)])
    raw=pack(frames);name=f'input-{rate}.f32.gz';raw_sha,gzip_sha=store(name,raw);inputs[rate]=(raw,name,raw_sha,gzip_sha)
cases=[]
def case(name,values,rate=48000,role='development'):cases.append(dict(name=name,values=values,sample_rate=rate,role=role))
for rate in [44100,48000,96000]:
    for kind in [0,1]:
        for threshold in range(1,11):case(f'{rate}-type-{kind}-threshold-{threshold}',[128,threshold,kind,128,128],rate)
for kind in [0,1]:
    for threshold in [1,10]:
        for pre in [64,192]:
            for mix in [0,64,128]:
                for post in [0,64,128]:case(f'levels-{kind}-{threshold}-{pre}-{mix}-{post}',[pre,threshold,kind,mix,post])
case('held-out-96000',[157,7,1,103,91],96000,'held-out')
for c in cases:
    raw,name,input_sha,input_gzip_sha=inputs[c['sample_rate']];input_path=work/'input.f32';input_path.write_bytes(raw);out=work/'output.f32'
    subprocess.run([str(binary),str(input_path),str(out),','.join(map(str,c['values']))],check=True)
    expected=out.read_bytes();samples=struct.unpack('<'+'f'*(len(expected)//4),expected);assert all(map(math.isfinite,samples))
    c.update(input=name,input_sha256=input_sha,input_gzip_sha256=input_gzip_sha,output=c['name']+'-output.f32.gz',frames=len(raw)//8,block_frames=64,reset=True)
    c['output_sha256'],c['output_gzip_sha256']=store(c['output'],expected);c['peak']=max(map(abs,samples))
manifest=dict(repository=pin['repository'],commit=pin['commit'],sources=sources,compiler=subprocess.check_output([args.rustc,'--version'],text=True).strip(),reference_binary_sha256=sha(binary.read_bytes()),maximum_absolute_error=2e-7,alignment_frames=0,gain_correction=1.0,cases=cases)
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8');print(f'Generated {len(cases)} Fast Distortion reference cases')
