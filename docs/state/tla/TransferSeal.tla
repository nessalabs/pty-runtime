---- MODULE TransferSeal ----
EXTENDS Naturals
VARIABLES seq, sealed, boundary, drained, failed, sealDrain, sealFailure
vars == <<seq, sealed, boundary, drained, failed, sealDrain, sealFailure>>
Init == /\ seq = 0 /\ sealed = FALSE /\ boundary = 0 /\ drained = FALSE /\ failed = FALSE /\ sealDrain = FALSE /\ sealFailure = FALSE

Append == /\ seq < 2 /\ ~sealed /\ seq' = seq + 1 /\ UNCHANGED <<sealed, boundary, drained, failed, sealDrain, sealFailure>>
Drain == /\ ~drained /\ drained' = TRUE /\ UNCHANGED <<seq, sealed, boundary, failed, sealDrain, sealFailure>>
Fail == /\ ~failed /\ failed' = TRUE /\ UNCHANGED <<seq, sealed, boundary, drained, sealDrain, sealFailure>>
Seal == /\ ~sealed /\ (drained \/ failed) /\ sealed' = TRUE /\ boundary' = seq
        /\ sealDrain' = drained /\ sealFailure' = failed /\ UNCHANGED <<seq, drained, failed>>

Next == Append \/ Drain \/ Fail \/ Seal
Spec == Init /\ [][Next]_vars
FrozenPrefix == sealed => seq = boundary
EndHasEvidence == sealed => (sealDrain \/ sealFailure)
====
