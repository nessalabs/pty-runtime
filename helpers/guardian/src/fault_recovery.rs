//! Single-threaded process fixtures preserve the helper's fork preconditions.
#![allow(
    dead_code,
    unused_imports,
    reason = "Imported helper modules also contain libtest tests and operations covered by the other declared targets"
)]
#[cfg(not(test))]
compile_error!("fault recovery fixtures require Cargo's test configuration");

mod anchor;
mod cancellation;
mod cleanup;
mod discovery;
#[path = "../tests/fault_fixture.rs"]
mod fault_fixture;
#[path = "../tests/fault_injection.rs"]
mod fault_injection;
mod links;
mod os;
#[path = "../../../scripts/guardian/protocol.rs"]
mod protocol;
#[path = "../tests/fault_recovery.rs"]
mod scenarios;
mod successor;
mod workload;

fn main() {
    scenarios::run();
}
