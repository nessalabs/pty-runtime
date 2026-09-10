//! Store format independence: envelope validation belongs to the protector.
use pty_runtime_application::checkpoint::ICheckpointStore;
use pty_runtime_domain::{
    ReplayCursor, SessionLifetime,
    checkpoint::{CheckpointKey, ProtectedCheckpoint},
    terminal::{CheckpointDescriptor, CompatibilityId},
};
use pty_runtime_infrastructure::checkpoint::FileCheckpointStore;

#[test]
fn storage_roundtrips_short_opaque_envelope_without_cipher_assumptions() {
    let mut unique = [0u8; 16];
    getrandom::getrandom(&mut unique).unwrap();
    let path = std::env::temp_dir().join(format!(
        "pty-envelope-review-{:032x}",
        u128::from_le_bytes(unique)
    ));
    let store = FileCheckpointStore::new(&path, 32).unwrap();
    let lifetime = SessionLifetime::new(940, 1);
    let checkpoint = ProtectedCheckpoint::new(
        CheckpointKey {
            lifetime,
            generation: 1,
        },
        CheckpointDescriptor {
            compatibility: CompatibilityId::new("independent-envelope-fixture").unwrap(),
            processed: ReplayCursor {
                lifetime,
                offset: 2,
            },
            control_generation: 0,
        },
        vec![0x17; 17],
    );
    let reference = store.commit(&checkpoint).unwrap();
    assert_eq!(reference.bytes, 17);
    let returned = store.read(reference, 17).unwrap();
    assert_eq!(returned.key, checkpoint.key);
    assert_eq!(returned.descriptor, checkpoint.descriptor);
    assert_eq!(returned.ciphertext(), checkpoint.ciphertext());
    store.delete(reference).unwrap();
    assert_eq!(store.capacity().committed_bytes, 0);
    drop(store);
    assert!(!path.exists());
}
