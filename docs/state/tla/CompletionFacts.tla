---- MODULE CompletionFacts ----
EXTENDS Naturals
VARIABLES actual, failed, drained, finished, reportedExit
vars == <<actual, failed, drained, finished, reportedExit>>
Init == /\ actual = FALSE /\ failed = FALSE /\ drained = FALSE /\ finished = FALSE /\ reportedExit = FALSE

ObserveExit == /\ ~actual /\ ~finished /\ actual' = TRUE /\ UNCHANGED <<failed, drained, finished, reportedExit>>
ObserveFailure == /\ ~failed /\ ~finished /\ failed' = TRUE /\ UNCHANGED <<actual, drained, finished, reportedExit>>
Drain == /\ ~drained /\ drained' = TRUE /\ UNCHANGED <<actual, failed, finished, reportedExit>>
Finish == /\ ~finished /\ (actual \/ failed) /\ drained
          /\ finished' = TRUE /\ reportedExit' = actual
          /\ UNCHANGED <<actual, failed, drained>>

Next == ObserveExit \/ ObserveFailure \/ Drain \/ Finish
Spec == Init /\ [][Next]_vars
BothFacts == finished => ((actual \/ failed) /\ drained)
NoInventedExit == reportedExit => actual
====
