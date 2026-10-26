"""Retain complete original Delay reference renders; native tests handle sinf differences."""
import argparse,gzip,hashlib,io,json,math,struct,subprocess,sys
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('reference_root',type=Path);parser.add_argument('--rustc',default='rustc');args=parser.parse_args()
repo=Path(__file__).resolve().parents[2];source=args.reference_root.resolve();work=repo/'.tools/reference-delay';work.mkdir(parents=True,exist_ok=True);output=repo/'tests/fixtures/processing/delay';output.mkdir(parents=True,exist_ok=True)
pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text());sha=lambda b:hashlib.sha256(b).hexdigest();relative='crates/os-engine/src/effects/delay.rs';source_sha=sha((source/relative).read_bytes());assert source_sha==pin['sha256'][relative];assert sha((repo/'tools/effect-reference/pinned/delay.rs').read_bytes())==pin['retained_sha256'][relative]
main=work/'main.rs';main.write_text(Path(__file__).with_name('delay.rs.template').read_text().replace('@ROOT@',source.as_posix()));binary=work/('reference.exe' if sys.platform=='win32' else 'reference');subprocess.run([args.rustc,'--edition=2024','-O',str(main),'-o',str(binary)],check=True)
def store(name,raw):
    out=io.BytesIO()
    with gzip.GzipFile(fileobj=out,filename='',mode='wb',mtime=0) as f:f.write(raw)
    packed=out.getvalue();(output/name).write_bytes(packed);return sha(raw),sha(packed)
base=[6000,1,1,0,192,3000,500,6000,3000,0,0,0,0,1,3000,0,6000,0,0,0,0,60000,6000,6000,0,6000]
cases=[]
for rate in [44100,48000,96000]:
    for model in range(4):
        v=base.copy();v[1]=model;v[2]=0;v[4]=160;v[25]=0;v[14]=3900
        cases.append(dict(name=f'{rate}-model-{model}',values=v,sample_rate=rate,bpm=120.0,frames=rate,role='development'))
v=base.copy();v[4]=768;v[25]=0;v[14]=0;v[6]=0
cases.append(dict(name='twelve-second-echo',values=v,sample_rate=48000,bpm=20.0,frames=600000,role='development'))
cases.append(dict(name='held-out-96000',values=[4200,2,1,1,17,900,730,4800,1571,4600,1700,3710,2910,1,3300,2,3940,2130,4200,2300,800,42000,4920,5000,-1777,1900],sample_rate=96000,bpm=137.5,frames=288000,role='held-out'))
for c in cases:
    frames=[];rate=c['sample_rate']
    for i in range(c['frames']):
        t=i/rate
        frames.extend([.8,-.3] if i==0 else [.4*math.sin(t*731)+.15*math.sin(t*1937),.32*math.cos(t*1231)] if i<rate//8 else [0.,0.])
    raw=struct.pack('<'+'f'*len(frames),*frames);input_path=work/'input.f32';input_path.write_bytes(raw);out=work/'output.f32'
    subprocess.run([str(binary),str(input_path),str(out),str(rate),str(c['bpm']),','.join(map(str,c['values']))],check=True)
    expected=out.read_bytes();samples=struct.unpack('<'+'f'*(len(expected)//4),expected);assert all(map(math.isfinite,samples))
    c.update(input=c['name']+'-input.f32.gz',output=c['name']+'-output.f32.gz',reset=True,preroll_frames=8192)
    c['input_sha256'],c['input_gzip_sha256']=store(c['input'],raw);c['output_sha256'],c['output_gzip_sha256']=store(c['output'],expected);c['peak']=max(map(abs,samples))
manifest=dict(repository=pin['repository'],commit=pin['commit'],sources={relative:source_sha},compiler=subprocess.check_output([args.rustc,'--version'],text=True).strip(),platform=sys.platform,reference_binary_sha256=sha(binary.read_bytes()),maximum_absolute_error=2e-6,maximum_relative_rms_error=2e-5,alignment_frames=0,gain_correction=1.0,native_sine_policy='Compare modulated PCM against unchanged source natively on every OS; retained captures document Windows provenance.',cases=cases)
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(f'Generated {len(cases)} Delay reference captures')
