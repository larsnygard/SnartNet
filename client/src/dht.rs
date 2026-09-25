//! Signed BEP-44 records for small, mutable discovery descriptors.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use mainline::{Dht, MutableItem, SigningKey};
use snartnet_core::KeyPair;
use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DhtStatus {
    pub bootstrapped: bool,
    pub last_lookup: Option<String>,
    pub last_publish: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DhtNode {
    node: Arc<Dht>,
    signing_key: SigningKey,
    status: Arc<Mutex<DhtStatus>>,
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
        builder.bind_address(Ipv4Addr::UNSPECIFIED);
        // A port that cannot be bound must not take the caller down. mainline 8.0
        // answers the startup `Check` only if that message already reached its
        // actor thread (`run` in mainline's `dht.rs`), so losing that race turns
        // a bind error into a panic reading "actor thread unexpectedly shutdown".
        // Probe the port first, and keep the remaining race from escaping.
        if port != 0 && port_is_bindable(port) {
            builder.port(port);
        }
        if let Some(bootstrap) = configured_bootstrap("SNARTNET_DHT_BOOTSTRAP") {
            builder.bootstrap(&bootstrap);
        } else if let Some(extra) = configured_bootstrap("SNARTNET_DHT_EXTRA_BOOTSTRAP") {
            builder.extra_bootstrap(&extra);
        }
        let node = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.build()))
            .map_err(|_| "DHT startup failed: port unavailable".to_string())?
            .map_err(|e| format!("DHT startup failed: {e}"))?;
        Ok(Self {
            node: Arc::new(node),
            signing_key,
            status: Arc::new(Mutex::new(DhtStatus::default())),
        })
    }

    pub fn status(&self) -> DhtStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
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
        if let Err(error) = result {
            if let Ok(mut status) = self.status.lock() {
                status.last_error = Some(error.clone());
            }
            return Err(error);
        }
        if let Ok(mut status) = self.status.lock() {
            status.bootstrapped = true;
            status.last_publish = Some(chrono::Utc::now().to_rfc3339());
            status.last_error = None;
        }
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
        if let Ok(mut status) = self.status.lock() {
            status.bootstrapped = true;
            status.last_lookup = Some(chrono::Utc::now().to_rfc3339());
            status.last_error = None;
        }
        Ok(item)
    }
}

fn configured_bootstrap(name: &str) -> Option<Vec<String>> {
    let values = std::env::var(name)
        .ok()?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

/// Whether `port` is bindable on all interfaces right now. The probe socket is
/// released again, so mainline binds the port itself when this returns `true`.
fn port_is_bindable(port: u16) -> bool {
    std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_that_is_already_taken_does_not_abort_startup() {
        let holder = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = holder.local_addr().unwrap().port();
        let keypair = KeyPair::generate().unwrap();
        // Degrade instead of panicking: concurrent sessions and other SnartNet
        // instances contend for the same auxiliary ports.
        let node = DhtNode::open(&keypair, port).expect("startup must not abort");
        assert!(!node.status().bootstrapped);
    }
}
