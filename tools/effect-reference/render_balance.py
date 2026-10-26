"""Retain complete pinned original Balance renders, outside application dependencies."""
import argparse, gzip, hashlib, json, math, struct, subprocess, sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('reference_root',type=Path);a=p.parse_args()
repo=Path(__file__).resolve().parents[2];work=repo/'.tools/reference-balance';work.mkdir(parents=True,exist_ok=True);out=repo/'tests/fixtures/processing/balance';out.mkdir(parents=True,exist_ok=True)
sha=lambda b:hashlib.sha256(b).hexdigest()
source=a.reference_root.resolve();rel='crates/os-engine/src/effects/balance.rs';pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text());assert sha((source/rel).read_bytes())==pin['sha256'][rel];assert sha((repo/'tools/effect-reference/pinned/balance.rs').read_bytes())==pin['retained_sha256'][rel]
main=work/'main.rs';main.write_text(Path(__file__).with_name('balance.rs.template').read_text().replace('@ROOT@',source.as_posix()));binary=work/('reference.exe' if sys.platform=='win32' else 'reference');subprocess.run(['rustc','--edition=2024','-O',str(main),'-o',str(binary)],check=True)
cases=[]
for name,b,v in [('centre',0,256),('left',-128,256),('right',128,256),('mute',0,0),('maximum',0,320),('held-out',-43,283)]:
    values=[]
    for i in range(1024):values.extend([1.,0.] if i==0 else [0.,1.] if i==1 else [1.,1.] if i==2 else [1.,-1.] if i==3 else [math.sin(i*.271),math.cos(i*.193)*.7])
    raw=struct.pack('<'+'f'*len(values),*values);(work/'input').write_bytes(raw);subprocess.run([str(binary),str(work/'input'),str(work/'output'),str(b),str(v)],check=True);render=(work/'output').read_bytes()
    c={'name':name,'balance':b,'volume':v,'frames':1024,'sample_rate':48000,'channels':2,'input':name+'-input.f32.gz','output':name+'-output.f32.gz'}
    for key,data in [('input',raw),('output',render)]:
        packed=gzip.compress(data,mtime=0);(out/c[key]).write_bytes(packed);c[key+'_sha256']=sha(data);c[key+'_gzip_sha256']=sha(packed)
    cases.append(c)
(out/'manifest.json').write_text(json.dumps({'commit':'0af860dfd31c7ee9cbf8af6747617e031a1a7742','source_sha256':sha((source/rel).read_bytes()),'compiler':subprocess.check_output(['rustc','--version'],text=True).strip(),'reference_binary_sha256':sha(binary.read_bytes()),'absolute_tolerance':2e-6,'relative_rms_tolerance':2e-5,'gain_correction':1,'alignment_frames':0,'cases':cases},indent=2)+'\n')
print('Generated six complete Balance captures')
