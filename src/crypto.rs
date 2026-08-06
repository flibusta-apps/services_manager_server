use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::config::CONFIG;

const ENC_INFO: &[u8] = b"services_manager_server/token-encryption/v1";
const HMAC_INFO: &[u8] = b"services_manager_server/token-hmac/v1";

struct TokenCrypto {
    enc_key: Key<Aes256Gcm>,
    hmac_key: [u8; 32],
}

fn derive_subkeys(master: &[u8]) -> ([u8; 32], [u8; 32]) {
    let hk = Hkdf::<Sha256>::new(None, master);
    let mut enc_key = [0u8; 32];
    let mut hmac_key = [0u8; 32];
    hk.expand(ENC_INFO, &mut enc_key)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    hk.expand(HMAC_INFO, &mut hmac_key)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    (enc_key, hmac_key)
}

static TOKEN_CRYPTO: once_cell::sync::Lazy<TokenCrypto> = once_cell::sync::Lazy::new(|| {
    let master = STANDARD
        .decode(CONFIG.token_enc_key.trim())
        .expect("TOKEN_ENC_KEY must be valid base64");
    assert_eq!(
        master.len(),
        32,
        "TOKEN_ENC_KEY must decode to exactly 32 bytes"
    );
    let (enc_key, hmac_key) = derive_subkeys(&master);
    TokenCrypto {
        enc_key: *Key::<Aes256Gcm>::from_slice(&enc_key),
        hmac_key,
    }
});

pub struct EncryptedToken {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

pub fn encrypt_token(plaintext: &str) -> EncryptedToken {
    let cipher = Aes256Gcm::new(&TOKEN_CRYPTO.enc_key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .expect("AES-256-GCM encryption must not fail");
    EncryptedToken {
        ciphertext,
        nonce: nonce.to_vec(),
    }
}

pub fn decrypt_token(ciphertext: &[u8], nonce: &[u8]) -> Result<String, String> {
    let cipher = Aes256Gcm::new(&TOKEN_CRYPTO.enc_key);
    let nonce = Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "failed to decrypt token".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "decrypted token is not valid UTF-8".to_string())
}

pub fn hmac_token(plaintext: &str) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&TOKEN_CRYPTO.hmac_key)
        .expect("HMAC accepts any key size");
    mac.update(plaintext.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests exercise `derive_subkeys` directly since it's a pure
    // function with no dependency on the env-backed `CONFIG`/`TOKEN_CRYPTO`
    // lazy statics, which would require `TOKEN_ENC_KEY` to be set process-wide
    // before any test runs. Full encrypt/decrypt/hmac round-trip coverage
    // through the real `TOKEN_CRYPTO` singleton is exercised by the
    // integration smoke test against a running server instead.

    #[test]
    fn derive_subkeys_is_deterministic() {
        let master = [7u8; 32];
        let (enc1, hmac1) = derive_subkeys(&master);
        let (enc2, hmac2) = derive_subkeys(&master);
        assert_eq!(enc1, enc2);
        assert_eq!(hmac1, hmac2);
    }

    #[test]
    fn derive_subkeys_enc_and_hmac_differ() {
        let master = [7u8; 32];
        let (enc, hmac) = derive_subkeys(&master);
        assert_ne!(enc, hmac);
    }

    #[test]
    fn derive_subkeys_differs_for_different_master_keys() {
        let master_a = [1u8; 32];
        let master_b = [2u8; 32];
        let (enc_a, hmac_a) = derive_subkeys(&master_a);
        let (enc_b, hmac_b) = derive_subkeys(&master_b);
        assert_ne!(enc_a, enc_b);
        assert_ne!(hmac_a, hmac_b);
    }

    #[test]
    fn encrypt_decrypt_round_trip_with_manual_key() {
        // Exercise the AES-GCM encrypt/decrypt primitives directly (not via
        // the CONFIG-backed singleton) to keep this test independent of env vars.
        let master = [9u8; 32];
        let (enc_key, _hmac_key) = derive_subkeys(&master);
        let key = *Key::<Aes256Gcm>::from_slice(&enc_key);
        let cipher = Aes256Gcm::new(&key);

        let plaintext = "super-secret-bot-token";

        let nonce1 = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct1 = cipher.encrypt(&nonce1, plaintext.as_bytes()).unwrap();
        let nonce2 = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct2 = cipher.encrypt(&nonce2, plaintext.as_bytes()).unwrap();

        // Random nonces => different ciphertexts/nonces for identical plaintext.
        assert_ne!(nonce1.to_vec(), nonce2.to_vec());
        assert_ne!(ct1, ct2);

        let decrypted = cipher.decrypt(&nonce1, ct1.as_ref()).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), plaintext);
    }

    #[test]
    fn hmac_is_deterministic_and_distinguishes_inputs_manual_key() {
        let master = [3u8; 32];
        let (_enc_key, hmac_key) = derive_subkeys(&master);

        let mac_for = |input: &str| {
            let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&hmac_key).unwrap();
            mac.update(input.as_bytes());
            mac.finalize().into_bytes().to_vec()
        };

        let a1 = mac_for("token-a");
        let a2 = mac_for("token-a");
        let b = mac_for("token-b");

        assert_eq!(a1, a2);
        assert_ne!(a1, b);
    }
}
