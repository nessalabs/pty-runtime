import json, subprocess, time
from pathlib import Path
root=Path('/home/user/Documents/pty-runtime-candidate6')
out=Path('/home/user/Documents/candidate6-spawn-trace')
out.mkdir(exist_ok=False)
cmd=['/usr/bin/strace','-f','-e','trace=process','-e','status=failed','-o']
binary=root/'target/debug/deps/raw_runtime-07b904908a54cf76'
started=time.time()
for n in range(1,201):
 with (out/f'{n}.log').open('w') as log:
  try:
   run=subprocess.run(cmd+[str(out/f'{n}.trace'),str(binary),'--test-threads=4'],cwd=root,stdout=log,stderr=subprocess.STDOUT,timeout=30)
   code=run.returncode
  except subprocess.TimeoutExpired:
   code='timeout'
 row={'trial':n,'exit_code':code,'elapsed_seconds':time.time()-started}
 with (out/'results.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
 print(json.dumps(row),flush=True)
 if code!=0:break
