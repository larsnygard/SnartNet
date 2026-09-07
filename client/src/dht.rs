//! Signed BEP-44 records for small, mutable discovery descriptors.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use mainline::{Dht, MutableItem, SigningKey};
use snartnet_core::KeyPair;
use std::{net::Ipv4Addr, sync::Arc};

#[derive(Clone, Debug)]
pub struct DhtNode {
    node: Arc<Dht>,
    signing_key: SigningKey,
}

impl DhtNode {
    pub fn open(keypair: &KeyPair, port: u16) -> Result<Self, String> {
        let secret = STANDARD
            .decode(&keypair.secret_key)
            .map_err(|e| format!("invalid signing key: {e}"))?;
        let bytes: [u8; 32] = secret
            .try_into()
            .map_err(|_| "invalid signing key length".to_string())?;
        let signing_key = SigningKey::from_bytes(&bytes);
        let mut builder = Dht::builder();
        builder.port(port).bind_address(Ipv4Addr::UNSPECIFIED);
        let node = builder
            .build()
            .map_err(|e| format!("DHT startup failed: {e}"))?;
        Ok(Self {
            node: Arc::new(node),
            signing_key,
        })
    }

    pub fn salt(namespace: &str, parts: &[&str]) -> Vec<u8> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(namespace.as_bytes());
        for part in parts {
            hasher.update(&[0]);
            hasher.update(part.as_bytes());
        }
        hasher.finalize().as_bytes()[..32].to_vec()
    }

    pub fn publish(&self, namespace: &str, parts: &[&str], value: &[u8]) -> Result<i64, String> {
        if value.len() > 1000 {
            return Err("DHT descriptor exceeds the BEP-44 size limit".into());
        }
        let salt = Self::salt(namespace, parts);
        let key = self.signing_key.verifying_key().to_bytes();
        #[allow(deprecated)]
        let current = self.node.get_mutable_most_recent(&key, Some(&salt));
        let seq = current.as_ref().map(|item| item.seq() + 1).unwrap_or(1);
        let item = MutableItem::new(self.signing_key.clone(), value, seq, Some(&salt));
        #[allow(deprecated)]
        let result = self
            .node
            .put_mutable(item, current.as_ref().map(|item| item.seq()))
            .map_err(|e| format!("DHT record publish failed: {e}"));
        result?;
        Ok(seq)
    }

    pub fn get_for(
        &self,
        public_key_b64: &str,
        namespace: &str,
        parts: &[&str],
    ) -> Result<Option<Vec<u8>>, String> {
        let public_key = STANDARD
            .decode(public_key_b64)
            .map_err(|e| format!("invalid DHT public key: {e}"))?;
        let public_key: [u8; 32] = public_key
            .try_into()
            .map_err(|_| "invalid DHT public key length".to_string())?;
        let salt = Self::salt(namespace, parts);
        #[allow(deprecated)]
        let item = self
            .node
            .get_mutable_most_recent(&public_key, Some(&salt))
            .map(|item| item.value().to_vec());
        Ok(item)
    }
}
