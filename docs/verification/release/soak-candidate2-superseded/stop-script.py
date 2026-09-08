import datetime,json,os,pathlib,select,signal,time
root=pathlib.Path('/home/user/Documents/pty-runtime-candidate2')
source=root/'docs/verification/release/soak-12h-candidate2.jsonl'
driver=165555
handle=os.pidfd_open(driver)
cmd=pathlib.Path(f'/proc/{driver}/cmdline').read_bytes().split(b'\0')
assert cmd[1:5]==[b'scripts/release/stress.py',b'soak',b'--output',b'docs/verification/release/soak-12h-candidate2.jsonl'],cmd
assert pathlib.Path(f'/proc/{driver}/cwd').resolve()==root
identity=json.loads(source.open().readline())
owner=identity['owner_pid']
def processes():
 result={}
 for path in pathlib.Path('/proc').iterdir():
  if not path.name.isdigit():continue
  try:
   tail=(path/'stat').read_text().rsplit(')',1)[1].split()
   result[int(path.name)]=(int(tail[1]),tail[0],tail[19])
  except (FileNotFoundError,ProcessLookupError,PermissionError):pass
 return result
before=processes();selected={driver}
while True:
 expanded=selected|{pid for pid,(parent,_,_) in before.items() if parent in selected}
 if expanded==selected:break
 selected=expanded
assert owner in selected,'recorded owner is not a current descendant of driver'
record={'reason':'Intentionally superseded by reviewed runtime control-admission correction 487ef08; partial run is not twelve-hour acceptance','stopped_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'driver':driver,'owner':owner,'tracked_processes':{str(pid):before[pid] for pid in selected if pid in before}}
# Target the verified process handle; its BaseException cleanup kills and reaps its own runtime child.
signal.pidfd_send_signal(handle,signal.SIGINT)
poller=select.poll();poller.register(handle,select.POLLIN);record['driver_exit_observed']=bool(poller.poll(30000));os.close(handle)
deadline=time.monotonic()+30
while True:
 after=processes();remaining={str(pid):after[pid] for pid in selected if pid in after and pid in before and after[pid][2]==before[pid][2]}
 if not remaining or time.monotonic()>=deadline:break
 time.sleep(.1)
record['remaining_original_processes']=remaining
record['last_record']=json.loads(source.read_text().splitlines()[-1])
(root/'docs/verification/release/soak-candidate2-superseded.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
raise SystemExit(0 if record['driver_exit_observed'] and not remaining else 1)
