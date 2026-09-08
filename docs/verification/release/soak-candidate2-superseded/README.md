# Superseded candidate-2 partial soak

This run was intentionally stopped at 2026-09-08 20:38:30 UTC because runtime
control-admission behavior changed in reviewed checkpoint 487ef08. It started
at 19:58:13 UTC and did not complete twelve hours. The final driver_failure
record results from the deliberate interrupt, not a claimed runtime defect.
The partial timeseries is retained; no twelve-hour or plateau acceptance is
claimed from it.

The stop script verified the driver command and working directory, used a Linux
process handle to interrupt that exact driver, and allowed its existing cleanup
to kill/reap the owned runtime. Original driver/runtime/helper/workload processes
were tracked by PID and start time; none remained afterward. See stop.json.
The next soak must record its own corrected source/build identity and full run.
