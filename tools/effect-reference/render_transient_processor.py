"""Retain complete pinned original Transient Processor renders, outside application dependencies."""
import argparse, gzip, hashlib, json, math, struct, subprocess, sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('reference_root',type=Path);a=p.parse_args()
repo=Path(__file__).resolve().parents[2];work=repo/'.tools/reference-transient_processor';work.mkdir(parents=True,exist_ok=True);out=repo/'tests/fixtures/processing/transient_processor';out.mkdir(parents=True,exist_ok=True)
sha=lambda b:hashlib.sha256(b).hexdigest()
source=a.reference_root.resolve();rel='crates/os-engine/src/effects/transient_processor.rs';pin=json.loads((repo/'.scratch/wavebob/reference-manifest.json').read_text());assert sha((source/rel).read_bytes())==pin['sha256'][rel];assert sha((repo/'tools/effect-reference/pinned/transient_processor.rs').read_bytes())==pin['retained_sha256'][rel]
main=work/'main.rs';main.write_text(Path(__file__).with_name('transient_processor.rs.template').read_text().replace('@ROOT@',source.as_posix()));binary=work/('reference.exe' if sys.platform=='win32' else 'reference');subprocess.run(['rustc','--edition=2024','-O',str(main),'-o',str(binary)],check=True)
cases=[]
for name,v,rate in [('default44100',[80,0,80,80,1,1,80,80,1],44100),('default48000',[80,0,80,80,1,1,80,80,1],48000),('default96000',[80,0,80,80,1,1,80,80,1],96000),('attack-boost',[160,16,80,80,0,1,80,80,1],48000),('attack-cut',[0,0,80,80,2,1,80,80,1],48000),('release-boost',[80,0,160,80,1,2,80,80,1],48000),('low-band',[160,160,0,160,2,0,0,0,1],48000),('high-drive',[160,160,160,160,2,0,160,160,1],48000),('off',[160,160,160,160,2,2,160,160,0],48000),('held-out',[127,93,41,113,2,0,51,129,1],96000)]:
    values=[]
    for i in range(1024):values.extend([1.,0.] if i==0 else [0.,1.] if i==1 else [1.,1.] if i==2 else [1.,-1.] if i==3 else [math.sin(i*.271),math.cos(i*.193)*.7])
    raw=struct.pack('<'+'f'*len(values),*values);(work/'input').write_bytes(raw);delay=int(subprocess.check_output([str(binary),str(work/'input'),str(work/'output'),','.join(map(str,v)),str(rate)],text=True));render=(work/'output').read_bytes()
    c={'name':name,'values':v,'delay_frames':delay,'frames':1024,'sample_rate':rate,'channels':2,'input':name+'-input.f32.gz','output':name+'-output.f32.gz'}
    for key,data in [('input',raw),('output',render)]:
        packed=gzip.compress(data,mtime=0);(out/c[key]).write_bytes(packed);c[key+'_sha256']=sha(data);c[key+'_gzip_sha256']=sha(packed)
    cases.append(c)
(out/'manifest.json').write_text(json.dumps({'commit':'0af860dfd31c7ee9cbf8af6747617e031a1a7742','source_sha256':sha((source/rel).read_bytes()),'compiler':subprocess.check_output(['rustc','--version'],text=True).strip(),'reference_binary_sha256':sha(binary.read_bytes()),'absolute_tolerance':2e-6,'relative_rms_tolerance':2e-5,'gain_correction':1,'alignment_frames':0,'cases':cases},indent=2)+'\n')
print('Generated ten complete Transient Processor captures')
