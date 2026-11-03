---- MODULE StatusCommit ----
EXTENDS Naturals
VARIABLES generation, plan, planOwner, planGeneration, nativeAccepted, committed, commitOwner, commitGeneration, commitCurrent, commitNative
vars == <<generation, plan, planOwner, planGeneration, nativeAccepted, committed, commitOwner, commitGeneration, commitCurrent, commitNative>>
Init == /\ generation = 0 /\ plan = FALSE /\ planOwner = 1 /\ planGeneration = 0 /\ nativeAccepted = FALSE /\ committed = FALSE /\ commitOwner = 1 /\ commitGeneration = 0 /\ commitCurrent = 0 /\ commitNative = FALSE

Scan == \E owner \in 1..2:
        /\ ~plan /\ ~committed /\ plan' = TRUE /\ planOwner' = owner /\ planGeneration' = generation
        /\ UNCHANGED <<generation, nativeAccepted, committed, commitOwner, commitGeneration, commitCurrent, commitNative>>
OtherCommit == /\ generation = 0 /\ generation' = 1 /\ UNCHANGED <<plan, planOwner, planGeneration, nativeAccepted, committed, commitOwner, commitGeneration, commitCurrent, commitNative>>
AcceptNative == /\ plan /\ ~nativeAccepted /\ nativeAccepted' = TRUE /\ UNCHANGED <<generation, plan, planOwner, planGeneration, committed, commitOwner, commitGeneration, commitCurrent, commitNative>>
Commit == /\ plan /\ ~committed /\ planOwner = 1 /\ planGeneration = generation /\ nativeAccepted
          /\ committed' = TRUE /\ commitOwner' = planOwner /\ commitGeneration' = planGeneration /\ commitCurrent' = generation /\ commitNative' = nativeAccepted
          /\ UNCHANGED <<generation, plan, planOwner, planGeneration, nativeAccepted>>

Next == Scan \/ OtherCommit \/ AcceptNative \/ Commit
Spec == Init /\ [][Next]_vars
LocalPlan == committed => commitOwner = 1
CurrentPlan == committed => commitGeneration = commitCurrent
WholeNativeAcceptance == committed => commitNative
====
