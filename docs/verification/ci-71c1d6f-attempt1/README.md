# CI 71c1d6f first attempt

PTY experiments, macOS gate and MSRV passed. Ubuntu gate failed at the first survivor spawn in process_failure_isolation::sentinel_loss_reclaims_known_descendants_and_preserves_another_session, returning ProcessError::Io before intentional helper damage. The concurrent guardian-loss case passed. Dedicated Linux candidate5 gate/build on the same293-source inventory passed separately. Cause remains unestablished; this failed attempt is preserved, not replaced by a later rerun.
