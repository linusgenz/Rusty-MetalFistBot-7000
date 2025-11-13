use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit, Payload as AesPayload},
};
use anyhow::Result;
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead as ChaChaAead, Payload},
};

#[derive(Clone)]
pub enum CipherMode {
    AES(Aes256Gcm),
    XChaCha(XChaCha20Poly1305),
}

impl CipherMode {
    pub fn from_secret_and_mode(secret_key: &[u8], mode: &str) -> Result<Self> {
        if secret_key.len() != 32 {
            return Err(anyhow::anyhow!("secret_key length is not 32"));
        }

        match mode {
            "aead_aes256_gcm_rtpsize" => {
                let key_ga = aes_gcm::Key::<aes_gcm::Aes256Gcm>::from_slice(secret_key);
                Ok(CipherMode::AES(Aes256Gcm::new(key_ga)))
            }
            "aead_xchacha20_poly1305_rtpsize" => {
                let key_ga = chacha20poly1305::Key::from_slice(secret_key);
                Ok(CipherMode::XChaCha(XChaCha20Poly1305::new(key_ga)))
            }
            _ => Err(anyhow::anyhow!("Unsupported cipher mode")),
        }
    }

    pub fn encrypt_packet(
        &self,
        rtp_header: &[u8],
        opus_payload: &[u8],
        counter: u32,
    ) -> Result<Vec<u8>> {
        match self {
            CipherMode::XChaCha(xchacha) => {
                let mut nonce24 = [0u8; 24];
                nonce24[..4].copy_from_slice(&counter.to_be_bytes());
                let payload = Payload {
                    aad: rtp_header,
                    msg: opus_payload,
                };
                let ciphertext = xchacha
                    .encrypt(chacha20poly1305::XNonce::from_slice(&nonce24), payload)
                    .map_err(|e| anyhow::anyhow!("XChaCha encryption failed: {:?}", e))?;
                let mut out = Vec::with_capacity(ciphertext.len() + 4);
                out.extend_from_slice(&ciphertext);
                out.extend_from_slice(&counter.to_be_bytes());
                Ok(out)
            }
            CipherMode::AES(aes) => {
                let mut nonce12 = [0u8; 12];
                nonce12[..4].copy_from_slice(&counter.to_be_bytes());
                let aad = AesPayload {
                    aad: rtp_header,
                    msg: opus_payload,
                };
                let ct = aes
                    .encrypt(aes_gcm::Nonce::from_slice(&nonce12), aad)
                    .map_err(|e| anyhow::anyhow!("AES-GCM encrypt failed: {:?}", e))?;
                let mut out = Vec::with_capacity(ct.len() + 4);
                out.extend_from_slice(&ct);
                out.extend_from_slice(&counter.to_be_bytes());
                Ok(out)
            }
        }
    }
}
