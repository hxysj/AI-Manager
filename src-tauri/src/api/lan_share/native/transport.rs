use super::*;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use ring::{agreement, hkdf, rand::SystemRandom};

pub(super) const VERSION: &str = "aes-256-gcm-v1";
pub(super) const MAX_BODY: usize = attachments::FILE_CHUNK_BYTES as usize + 256 * 1024 + 32;
const MAX_AGE: u64 = 5 * 60 * 1000;

#[derive(Clone)]
pub(super) struct TransportAuth {
    pub sender: String,
    pub receiver: String,
    pub secret: String,
    pub pairing_id: String,
}

impl TransportAuth {
    pub fn scope(&self) -> &str {
        if self.secret.is_empty() {
            "software"
        } else if self.pairing_id.is_empty() {
            "paired"
        } else {
            "pairing"
        }
    }

    pub fn aad(&self) -> String {
        format!(
            "{DISCOVERY_PROTOCOL}|request|{}|{}|{}|{}",
            self.sender,
            self.receiver,
            self.scope(),
            self.pairing_id
        )
    }
}

pub(super) fn keypair() -> Result<(agreement::EphemeralPrivateKey, String), ManagerError> {
    let private =
        agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
            .map_err(|_| ManagerError::System("设备配对密钥生成失败。".into()))?;
    let public = private
        .compute_public_key()
        .map_err(|_| ManagerError::System("设备配对公钥生成失败。".into()))?;
    Ok((
        private,
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.as_ref()),
    ))
}

pub(super) fn pair_secret(
    private: agreement::EphemeralPrivateKey,
    remote_public: &str,
    initiator: &str,
    receiver: &str,
    request_id: &str,
    initiator_public: &str,
    receiver_public: &str,
) -> Result<String, ManagerError> {
    let public = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(remote_public)
        .map_err(|_| ManagerError::System("设备配对公钥无效。".into()))?;
    let transcript = format!("{DISCOVERY_PROTOCOL}|{initiator}|{receiver}|{request_id}|{initiator_public}|{receiver_public}");
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, transcript.as_bytes());
    let remote = agreement::UnparsedPublicKey::new(&agreement::X25519, public);
    let key = agreement::agree_ephemeral(private, &remote, |shared| {
        let info = [b"device-drop-pairing-key".as_slice()];
        let prk = salt.extract(shared);
        let mut key = [0u8; 32];
        prk.expand(&info, hkdf::HKDF_SHA256)
            .and_then(|okm| okm.fill(&mut key))?;
        Ok::<_, ring::error::Unspecified>(key)
    })
    .map_err(|_| ManagerError::System("设备配对公钥无效。".into()))?
    .map_err(|_| ManagerError::System("设备配对密钥协商失败。".into()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key))
}

// 软件密钥用于协议识别；附件和消息使用 X25519 协商出的设备配对密钥。
pub(super) fn cipher(secret: &str) -> Aes256Gcm {
    let domain = if secret.is_empty() {
        "software"
    } else {
        "paired"
    };
    let seed = if secret.is_empty() {
        "p4fHs7xM2dQ9yNcV8bLe6WzR1tKa5Guj"
    } else {
        secret
    };
    let digest = Sha256::digest(format!("{DISCOVERY_PROTOCOL}|{domain}|{seed}"));
    Aes256Gcm::new_from_slice(&digest).expect("AES-256 密钥长度固定")
}

pub(super) fn encrypt(secret: &str, aad: &str, plaintext: &[u8]) -> Result<Vec<u8>, ManagerError> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|error| ManagerError::System(error.to_string()))?;
    let encrypted = cipher(secret)
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| ManagerError::System("设备快传加密失败。".into()))?;
    let mut result = Vec::with_capacity(12 + encrypted.len());
    result.extend_from_slice(&nonce);
    result.extend_from_slice(&encrypted);
    Ok(result)
}

pub(super) fn decrypt(secret: &str, aad: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ManagerError> {
    if ciphertext.len() < 28 || ciphertext.len() > MAX_BODY {
        return Err(ManagerError::System("设备快传密文长度无效。".into()));
    }
    cipher(secret)
        .decrypt(
            Nonce::from_slice(&ciphertext[..12]),
            Payload {
                msg: &ciphertext[12..],
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| ManagerError::System("设备快传密钥不匹配或内容已被篡改。".into()))
}

pub(super) fn pack(metadata: &Value, bytes: &[u8]) -> Result<Vec<u8>, ManagerError> {
    let metadata = serde_json::to_vec(metadata)?;
    if metadata.len() > 256 * 1024 || bytes.len() > attachments::FILE_CHUNK_BYTES as usize {
        return Err(ManagerError::System("设备快传载荷过大。".into()));
    }
    let mut packet = Vec::with_capacity(4 + metadata.len() + bytes.len());
    packet.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
    packet.extend_from_slice(&metadata);
    packet.extend_from_slice(bytes);
    Ok(packet)
}

pub(super) fn unpack(packet: Vec<u8>) -> Result<(Value, Bytes), ManagerError> {
    if packet.len() < 4 {
        return Err(ManagerError::System("设备快传载荷无效。".into()));
    }
    let size = u32::from_be_bytes(packet[..4].try_into().unwrap()) as usize;
    if size > 256 * 1024 || size > packet.len() - 4 {
        return Err(ManagerError::System("设备快传元数据无效。".into()));
    }
    let metadata: Value = serde_json::from_slice(&packet[4..4 + size])?;
    Ok((metadata, Bytes::from(packet).slice(4 + size..)))
}

#[derive(Default)]
pub(super) struct ReplayGuard {
    seen: HashMap<String, u64>,
}

impl ReplayGuard {
    pub fn accept(&mut self, scope: &str, nonce: &[u8], sent_at: u64, now: u64) -> bool {
        if now.abs_diff(sent_at) > MAX_AGE {
            return false;
        }
        self.seen.retain(|_, expires| *expires > now);
        let id = format!(
            "{scope}:{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce)
        );
        if self.seen.contains_key(&id) || self.seen.len() >= 20_000 {
            return false;
        }
        self.seen.insert(id, sent_at.saturating_add(MAX_AGE + 1));
        true
    }
}

pub(super) async fn secure_call(
    client: &reqwest::Client,
    peer: &Peer,
    auth: &TransportAuth,
    operation: &str,
    payload: Value,
    bytes: &[u8],
    control: Option<&Arc<transfer::Control>>,
) -> Result<Value, ManagerError> {
    let origin = peer_origin(peer)?;
    let aad = auth.aad();
    let plaintext = pack(
        &json!({ "operation": operation, "payload": payload, "sentAt": now_millis() }),
        bytes,
    )?;
    let encrypted = encrypt(&auth.secret, &aad, &plaintext)?;
    let request_nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&encrypted[..12]);
    let content_length = encrypted.len();
    let body = match control {
        Some(control) => reqwest::Body::wrap_stream(control.bytes_stream(Bytes::from(encrypted))),
        None => reqwest::Body::from(encrypted),
    };
    let response = client
        .post(format!("{origin}/api/native/secure"))
        .header("x-lan-version", VERSION)
        .header("x-lan-sender", &auth.sender)
        .header("x-lan-receiver", &auth.receiver)
        .header("x-lan-scope", auth.scope())
        .header("x-lan-pairing", &auth.pairing_id)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_LENGTH, content_length)
        .body(body)
        .send()
        .await
        .map_err(network_error)?;
    let status = response.status();
    if response
        .headers()
        .get("x-lan-version")
        .and_then(|value| value.to_str().ok())
        != Some(VERSION)
    {
        return Err(ManagerError::System(
            "对方未通过加密认证，请更新双方软件并重新配对。".into(),
        ));
    }
    let mut content = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(bytes) = stream.next().await {
        let bytes = bytes.map_err(network_error)?;
        if content.len() + bytes.len() > 256 * 1024 {
            return Err(ManagerError::System("对方的加密响应过大。".into()));
        }
        content.extend_from_slice(&bytes);
    }
    let plaintext = decrypt(
        &auth.secret,
        &format!("{aad}|response|{request_nonce}"),
        &content,
    )?;
    let response: Value = serde_json::from_slice(&plaintext)?;
    if !status.is_success() || response["status"] != "success" {
        return Err(ManagerError::System(string_value(response.get("message"))));
    }
    Ok(response["data"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_rejects_wrong_keys_tampering_context_and_short_packets() {
        let plaintext = b"\xe7\xa7\x81\xe5\xaf\x86 message and image bytes";
        let packet = encrypt("paired-secret", "request-a", plaintext).unwrap();
        assert_eq!(
            decrypt("paired-secret", "request-a", &packet).unwrap(),
            plaintext
        );
        assert!(decrypt("other-secret", "request-a", &packet).is_err());
        assert!(decrypt("", "request-a", &packet).is_err());
        assert!(decrypt("paired-secret", "request-b", &packet).is_err());
        let mut modified = packet.clone();
        modified[15] ^= 1;
        assert!(decrypt("paired-secret", "request-a", &modified).is_err());
        assert!(decrypt("paired-secret", "request-a", &packet[..10]).is_err());
        assert_ne!(
            packet,
            encrypt("paired-secret", "request-a", plaintext).unwrap()
        );
        assert!(!packet
            .windows(plaintext.len())
            .any(|bytes| bytes == plaintext));
    }

    #[test]
    fn replay_guard_rejects_repeats_expired_and_future_requests() {
        let mut guard = ReplayGuard::default();
        assert!(guard.accept("peer", b"nonce", 1_000_000, 1_000_000));
        assert!(!guard.accept("peer", b"nonce", 1_000_000, 1_000_001));
        assert!(!guard.accept("peer", b"nonce", 1_000_000, 1_300_001));
        assert!(!guard.accept("peer", b"new", 2_000_000, 1_000_000));
    }

    #[test]
    fn x25519_agreement_matches_on_both_sides_and_binds_pairing_context() {
        let (initiator_private, initiator_public) = keypair().unwrap();
        let (receiver_private, receiver_public) = keypair().unwrap();
        let first = pair_secret(
            initiator_private,
            &receiver_public,
            "native-a",
            "native-b",
            "pair-a",
            &initiator_public,
            &receiver_public,
        )
        .unwrap();
        let second = pair_secret(
            receiver_private,
            &initiator_public,
            "native-a",
            "native-b",
            "pair-a",
            &initiator_public,
            &receiver_public,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(first)
                .unwrap()
                .len(),
            32
        );

        let (initiator_private, initiator_public) = keypair().unwrap();
        let (receiver_private, receiver_public) = keypair().unwrap();
        let first = pair_secret(
            initiator_private,
            &receiver_public,
            "native-a",
            "native-b",
            "pair-a",
            &initiator_public,
            &receiver_public,
        )
        .unwrap();
        let second = pair_secret(
            receiver_private,
            &initiator_public,
            "native-a",
            "native-b",
            "pair-b",
            &initiator_public,
            &receiver_public,
        )
        .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn x25519_rejects_invalid_and_low_order_public_keys() {
        for remote in [
            "invalid".to_string(),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 32]),
        ] {
            let (private, public) = keypair().unwrap();
            assert!(pair_secret(
                private, &remote, "native-a", "native-b", "pair-a", &public, &remote
            )
            .is_err());
        }
    }

    #[test]
    fn encrypted_responses_are_bound_to_the_request_nonce() {
        let response = encrypt("paired-secret", "request|response|nonce-a", b"accepted").unwrap();
        assert!(decrypt("paired-secret", "request|response|nonce-b", &response).is_err());
        assert_eq!(
            decrypt("paired-secret", "request|response|nonce-a", &response).unwrap(),
            b"accepted"
        );
    }
}
