"""Retain complete original Chorus reference renders; native tests handle sinf differences."""
import argparse,gzip,hashlib,io,json,math,struct,subprocess,sys
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('reference_root',type=Path);parser.add_argument('--rustc',default='rustc');args=parser.parse_args()
repo=Path(__file__).resolve().parents[2];source=args.reference_root.resolve();work=repo/'.tools/reference-chorus';work.mkdir(parents=True,exist_ok=True);output=repo/'tests/fixtures/processing/chorus';output.mkdir(parents=True,exist_ok=True)
pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text());sha=lambda b:hashlib.sha256(b).hexdigest();relative='crates/os-engine/src/effects/chorus.rs';source_sha=sha((source/relative).read_bytes());assert source_sha==pin['sha256'][relative];assert sha((repo/'tools/effect-reference/pinned/chorus.rs').read_bytes())==source_sha
main=work/'main.rs';main.write_text(Path(__file__).with_name('chorus.rs.template').read_text().replace('@ROOT@',source.as_posix()));binary=work/('reference.exe' if sys.platform=='win32' else 'reference');subprocess.run([args.rustc,'--edition=2024','-O',str(main),'-o',str(binary)],check=True)
def store(name,raw):
    out=io.BytesIO()
    with gzip.GzipFile(fileobj=out,filename='',mode='wb',mtime=0) as f:f.write(raw)
    packed=out.getvalue();(output/name).write_bytes(packed);return sha(raw),sha(packed)
base=[512,2250,341,1500,2500,3500,23,0,0,0,2500,0]
cases=[]
for rate in [44100,48000,96000]:
    cases.append(dict(name=f'default-{rate}',values=base.copy(),sample_rate=rate,frames=rate,role='development'))
    for shape,raw in enumerate([0,342,683,1024]):
        v=base.copy();v[0]=900;v[1]=4800;v[6:9]=[raw]*3;v[9]=100 if shape%2 else 0;v[11]=1024
        cases.append(dict(name=f'shape-{shape}-{rate}',values=v,sample_rate=rate,frames=rate,role='development'))
cases.append(dict(name='held-out-96000',values=[183,3791,719,2173,4931,971,502,1007,321,79,3981,893],sample_rate=96000,frames=96000,role='held-out'))
for c in cases:
    frames=[];rate=c['sample_rate']
    for i in range(c['frames']):
        t=i/rate
        frames.extend([.8,-.2] if i==0 else [.2,-.3] if i<128 else [.5*math.sin(t*3137)+.1*math.sin(t*17131),.4*math.cos(t*7531)] if i<rate//3 else [0.,0.])
    raw=struct.pack('<'+'f'*len(frames),*frames);input_path=work/'input.f32';input_path.write_bytes(raw);out=work/'output.f32'
    subprocess.run([str(binary),str(input_path),str(out),str(rate),','.join(map(str,c['values']))],check=True)
    expected=out.read_bytes();samples=struct.unpack('<'+'f'*(len(expected)//4),expected);assert all(map(math.isfinite,samples))
    c.update(input=c['name']+'-input.f32.gz',output=c['name']+'-output.f32.gz',reset=True,seed=0,block_frames=64)
    c['input_sha256'],c['input_gzip_sha256']=store(c['input'],raw);c['output_sha256'],c['output_gzip_sha256']=store(c['output'],expected);c['peak']=max(map(abs,samples))
manifest=dict(repository=pin['repository'],commit=pin['commit'],sources={relative:source_sha},compiler=subprocess.check_output([args.rustc,'--version'],text=True).strip(),platform=sys.platform,reference_binary_sha256=sha(binary.read_bytes()),maximum_absolute_error=2e-6,maximum_relative_rms_error=2e-5,alignment_frames=0,gain_correction=1.0,native_sine_policy='Native original tests compare quantized sine tables without platform tolerance inflation.',cases=cases)
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(f'Generated {len(cases)} Chorus reference captures')
