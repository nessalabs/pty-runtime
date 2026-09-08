import json
from pathlib import Path
pids=[260387, 260396, 260397, 260398, 260401, 260402, 260403, 260406, 260407, 260408, 260411, 260412, 260413, 260416, 260417, 260418, 260421, 260422, 260423, 260426, 260427, 260428, 260431, 260432, 260433]
print(json.dumps({"sampled_pids":pids,"still_present":[pid for pid in pids if Path(f"/proc/{pid}").exists()],"scope":"read-only absence check of previously sampled PIDs; no start-time identity retained by original collector"}))
