---- MODULE ReplayAdmission ----
EXTENDS Naturals
VARIABLES end, accepted, readLife, readOffset, readFloor, kind
vars == <<end, accepted, readLife, readOffset, readFloor, kind>>
Init == /\ end = 0 /\ accepted = FALSE /\ readLife = 1 /\ readOffset = 0 /\ readFloor = 0 /\ kind = "none"

Floor == IF end > 2 THEN end - 2 ELSE 0
Append == /\ end < 3 /\ end' = end + 1 /\ UNCHANGED <<accepted, readLife, readOffset, readFloor, kind>>
Read == \E life \in 1..2, offset \in 0..3:
        /\ life = 1 /\ offset <= end
        /\ accepted' = TRUE /\ readLife' = life /\ readOffset' = offset /\ readFloor' = Floor
        /\ kind' = IF offset < Floor THEN "gap" ELSE "dataOrPending"
        /\ UNCHANGED end

Next == Append \/ Read
Spec == Init /\ [][Next]_vars
SameLifetime == accepted => readLife = 1
GapBeforeSuffix == (accepted /\ readOffset < readFloor) => kind = "gap"
====
