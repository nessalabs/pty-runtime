use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce, aead::AeadInPlace};
use pty_runtime_application::checkpoint::ICheckpointProtector;
use pty_runtime_domain::{
    checkpoint::*,
    terminal::{CheckpointDescriptor, TerminalCheckpoint},
};

/// RustCrypto XChaCha20-Poly1305 with a random owner-lifetime key and fresh 192-bit nonces.
/// Key material is zeroized by the cipher's Drop implementation. No restart recovery.
/// The maximum plaintext bound is enforced before encryption or decryption allocation.
pub struct CheckpointProtector {
    cipher: XChaCha20Poly1305,
    max_bytes: usize,
}
impl CheckpointProtector {
    /// Obtain a new owner key from the OS CSPRNG. Never exposes key material.
    pub fn new(max_bytes: usize) -> Result<Self, CheckpointError> {
        if max_bytes == 0 || max_bytes > 512 * 1024 * 1024 {
            return Err(CheckpointError::InvalidConfiguration);
        }
        let mut key = zeroize::Zeroizing::new([0u8; 32]);
        getrandom::getrandom(key.as_mut()).map_err(|_| CheckpointError::EntropyUnavailable)?;
        let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_ref()));
        Ok(Self { cipher, max_bytes })
    }
}
fn metadata(key: CheckpointKey, d: &CheckpointDescriptor) -> Result<Vec<u8>, CheckpointError> {
    if key.lifetime != d.processed.lifetime
        || d.compatibility.len() > 4096
        || d.compatibility.is_empty()
    {
        return Err(CheckpointError::InvalidConfiguration);
    }
    let mut bytes = Vec::with_capacity(64 + d.compatibility.len());
    bytes.extend_from_slice(b"pty-checkpoint-xchacha-v1");
    for n in [
        key.lifetime.owner(),
        key.lifetime.sequence(),
        key.generation,
        d.processed.offset,
        d.control_generation,
        d.compatibility.len() as u64,
    ] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend_from_slice(d.compatibility.as_bytes());
    Ok(bytes)
}
impl ICheckpointProtector for CheckpointProtector {
    fn protected_size_limit(&self, plaintext_bytes: usize) -> Result<usize, CheckpointError> {
        if plaintext_bytes > self.max_bytes {
            return Err(CheckpointError::CapacityExceeded);
        }
        plaintext_bytes
            .checked_add(40)
            .ok_or(CheckpointError::CapacityExceeded)
    }
    fn protect(
        &self,
        key: CheckpointKey,
        mut checkpoint: TerminalCheckpoint,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        // Take ownership before any fallible validation or entropy request. Zeroizing
        // clears the entire allocation (including spare capacity) on every error.
        let mut ciphertext = zeroize::Zeroizing::new(std::mem::take(&mut checkpoint.bytes));
        // Truncated terminal encodings may leave previous plaintext in spare capacity.
        // Clear that tail before ciphertext growth can reallocate the old allocation.
        use zeroize::Zeroize;
        ciphertext.spare_capacity_mut().zeroize();
        if ciphertext.len() > self.max_bytes {
            return Err(CheckpointError::CapacityExceeded);
        }
        let aad = metadata(key, &checkpoint.descriptor)?;
        let mut nonce = [0; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| CheckpointError::EntropyUnavailable)?;
        let tag = self
            .cipher
            .encrypt_in_place_detached(XNonce::from_slice(&nonce), &aad, ciphertext.as_mut_slice())
            .map_err(|_| CheckpointError::AuthenticationFailed)?;
        // Grow only after plaintext has become ciphertext; realloc must not leave a
        // discarded plaintext allocation outside the zeroizing buffer's ownership.
        ciphertext
            .try_reserve_exact(40)
            .map_err(|_| CheckpointError::CapacityExceeded)?;
        ciphertext.extend_from_slice(&tag);
        ciphertext.extend_from_slice(&nonce);
        Ok(ProtectedCheckpoint::new(
            key,
            checkpoint.descriptor.clone(),
            std::mem::take(&mut *ciphertext),
        ))
    }
    fn open(
        &self,
        key: CheckpointKey,
        descriptor: &CheckpointDescriptor,
        checkpoint: ProtectedCheckpoint,
    ) -> Result<TerminalCheckpoint, CheckpointError> {
        if checkpoint.key != key || &checkpoint.descriptor != descriptor {
            return Err(CheckpointError::AuthenticationFailed);
        }
        let ciphertext = checkpoint.ciphertext();
        if ciphertext.len() < 40 {
            return Err(CheckpointError::AuthenticationFailed);
        }
        if ciphertext.len() - 40 > self.max_bytes {
            return Err(CheckpointError::CapacityExceeded);
        }
        let aad = metadata(key, descriptor)?;
        let mut nonce = [0; 24];
        nonce.copy_from_slice(&ciphertext[ciphertext.len() - 24..]);
        let mut bytes = zeroize::Zeroizing::new(checkpoint.into_ciphertext());
        let encrypted_len = bytes.len() - 24;
        bytes.truncate(encrypted_len);
        self.cipher
            .decrypt_in_place(XNonce::from_slice(&nonce), &aad, &mut *bytes)
            .map_err(|_| CheckpointError::AuthenticationFailed)?;
        Ok(TerminalCheckpoint {
            descriptor: descriptor.clone(),
            bytes: std::mem::take(&mut *bytes),
        })
    }
}
