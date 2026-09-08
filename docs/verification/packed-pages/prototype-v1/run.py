import hashlib,json,platform,subprocess,time
from pathlib import Path
root=Path.cwd();out=root/'work/packed-pages/matrix';out.mkdir(exist_ok=False)
files=[root/'experiments/native'/p for p in ['native-memory.c','packed-pages.h','platform.h']]+[root/'work/experiment-cache/native-bin/native-memory']
identity={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
(out/'identity.json').write_text(json.dumps(dict(platform=platform.platform(),sources=identity),indent=2))
for repeat in range(5):
 for n in [1,32]:
  for varied in [0,1]:
   for compression in [0,1]:
    for mode in [1,3,4]:
     name=f'r{repeat}-n{n}-v{varied}-c{compression}-a{mode}'
     cmd=[str(files[-1]),str(n),'10000',str(varied),str(compression),'4','0',str(mode)]
     start=time.time()
     p=subprocess.run(cmd,capture_output=True,timeout=60)
     (out/(name+'.jsonl')).write_bytes(p.stdout)
     (out/(name+'.stderr')).write_bytes(p.stderr)
     (out/(name+'.meta.json')).write_text(json.dumps(dict(command=cmd,exit_code=p.returncode,seconds=time.time()-start)))
     if p.returncode: raise SystemExit(f'{name} failed {p.returncode}')
     print(name,flush=True)
assert identity=={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
