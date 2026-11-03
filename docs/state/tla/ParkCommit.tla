---- MODULE ParkCommit ----
EXTENDS Naturals
VARIABLES phase, activity, captured, published, closed, released, releaseActivity, releaseClosed
vars == <<phase, activity, captured, published, closed, released, releaseActivity, releaseClosed>>
Init == /\ phase = "resident" /\ activity = 0 /\ captured = 0 /\ published = FALSE /\ closed = FALSE /\ released = FALSE /\ releaseActivity = 0 /\ releaseClosed = FALSE

Begin == /\ phase = "resident" /\ ~closed /\ phase' = "parking" /\ captured' = activity
         /\ UNCHANGED <<activity, published, closed, released, releaseActivity, releaseClosed>>
Activity == /\ activity = 0 /\ ~released /\ activity' = 1 /\ UNCHANGED <<phase, captured, published, closed, released, releaseActivity, releaseClosed>>
Publish == /\ phase = "parking" /\ ~published /\ published' = TRUE /\ UNCHANGED <<phase, activity, captured, closed, released, releaseActivity, releaseClosed>>
Close == /\ ~closed /\ closed' = TRUE /\ UNCHANGED <<phase, activity, captured, published, released, releaseActivity, releaseClosed>>
Release == /\ phase = "parking" /\ published /\ activity = captured /\ ~closed
           /\ phase' = "parked" /\ released' = TRUE /\ releaseActivity' = activity /\ releaseClosed' = closed
           /\ UNCHANGED <<activity, captured, published, closed>>

Next == Begin \/ Activity \/ Publish \/ Close \/ Release
Spec == Init /\ [][Next]_vars
PublishedBeforeRelease == released => published
CurrentAttempt == released => releaseActivity = captured
OpenAtRelease == released => ~releaseClosed
====
