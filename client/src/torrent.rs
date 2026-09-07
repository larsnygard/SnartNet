//! BitTorrent/DHT content transport.
//!
//! The application protocol remains responsible for signatures and encryption.
//! This module only moves immutable bytes through a DHT-backed torrent session.

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    create_torrent, AddTorrent, AddTorrentOptions, CreateTorrentOptions, DhtSessionConfig,
    ListenerMode, ListenerOptions, Session, SessionOptions,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

const OBJECT_DIR: &str = "objects";
const DOWNLOAD_DIR: &str = "downloads";

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TorrentStatus {
    pub listening: bool,
    pub reachability: String,
    pub peer_count: u64,
    pub last_fetch: Option<String>,
    pub last_publish: Option<String>,
    pub last_error: Option<String>,
}

pub struct TorrentNode {
    runtime: Arc<Runtime>,
    session: Arc<Session>,
    root: PathBuf,
    status: Arc<Mutex<TorrentStatus>>,
}

impl TorrentNode {
    pub fn open(root: impl AsRef<Path>, listen_port: u16) -> Result<Self, String> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join(OBJECT_DIR)).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join(DOWNLOAD_DIR)).map_err(|e| e.to_string())?;
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .map_err(|e| format!("torrent runtime failed: {e}"))?,
        );
        let bootstrap_addrs = std::env::var("SNARTNET_TORRENT_BOOTSTRAP")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .filter(|values| !values.is_empty());
        let options = SessionOptions {
            ipv4_only: false,
            dht: Some(DhtSessionConfig {
                // librqbit supplies a rotating default bootstrap list when this is None.
                bootstrap_addrs,
                port: Some(listen_port),
                persistence: None,
            }),
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpAndUtp,
                listen_addr: ([0, 0, 0, 0, 0, 0, 0, 0], listen_port).into(),
                announce_port: Some(listen_port),
                ipv4_only: false,
                enable_upnp_port_forwarding: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let session = runtime
            .block_on(Session::new_with_opts(root.join(DOWNLOAD_DIR), options))
            .map_err(|e| format!("torrent session failed: {e:#}"))?;
        Ok(Self {
            runtime,
            session,
            root,
            status: Arc::new(Mutex::new(TorrentStatus {
                listening: true,
                reachability: "unknown".into(),
                ..Default::default()
            })),
        })
    }

    pub fn status(&self) -> TorrentStatus {
        let mut status = self.status.lock().map(|status| status.clone()).unwrap_or_default();
        let peers = self.session.stats_snapshot().peers;
        status.peer_count = u64::from(peers.live_tcp + peers.live_utp + peers.live_socks);
        status
    }

    /// Create a single-file torrent, seed it locally, and return its magnet.
    pub fn publish(&self, object_id: &str, bytes: &[u8]) -> Result<String, String> {
        validate_object_id(object_id)?;
        let path = self.root.join(OBJECT_DIR).join(format!("{object_id}.json"));
        if let Err(error) = std::fs::write(&path, bytes) {
            self.record_error(error.to_string());
            return Err(format!("torrent object write failed: {error}"));
        }
        let torrent = self
            .runtime
            .block_on(async {
                let spawner = BlockingSpawner::new(2);
                create_torrent(&path, CreateTorrentOptions::default(), &spawner).await
            })
            .map_err(|e| {
                self.record_error(format!("{e:#}"));
                format!("torrent metadata creation failed: {e:#}")
            })?;
        let torrent_bytes = torrent
            .as_bytes()
            .map_err(|e| format!("torrent encoding failed: {e:#}"))?;
        let magnet = format!("{}&dn=snartnet-{object_id}", torrent.as_magnet());
        self.runtime
            .block_on(self.session.add_torrent(
                AddTorrent::from_bytes(torrent_bytes),
                Some(AddTorrentOptions {
                    output_folder: Some(self.root.join(OBJECT_DIR).to_string_lossy().into_owned()),
                    overwrite: true,
                    ..Default::default()
                }),
            ))
            .map_err(|e| {
                self.record_error(format!("{e:#}"));
                format!("torrent seed failed: {e:#}")
            })?;
        if let Ok(mut status) = self.status.lock() {
            status.last_publish = Some(chrono::Utc::now().to_rfc3339());
            status.last_error = None;
        }
        Ok(magnet)
    }

    /// Download a content-addressed object from any available swarm peer.
    pub fn fetch(&self, magnet: &str, object_id: &str) -> Result<Vec<u8>, String> {
        validate_object_id(object_id)?;
        let path = self
            .root
            .join(DOWNLOAD_DIR)
            .join(format!("{object_id}.json"));
        if path.exists() {
            return std::fs::read(path).map_err(|e| format!("torrent object read failed: {e}"));
        }
        let handle = self
            .runtime
            .block_on(self.session.add_torrent(
                AddTorrent::from_url(magnet.to_owned()),
                Some(AddTorrentOptions {
                    output_folder: Some(
                        self.root.join(DOWNLOAD_DIR).to_string_lossy().into_owned(),
                    ),
                    overwrite: true,
                    ..Default::default()
                }),
            ))
            .map_err(|e| {
                self.record_error(format!("{e:#}"));
                format!("torrent download failed: {e:#}")
            })?
            .into_handle()
            .ok_or("torrent was not added")?;
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), handle.wait_until_completed())
                .await
                .map_err(|_| "torrent download timed out".to_string())?
                .map_err(|e| format!("torrent completion failed: {e:#}"))
        })
        .inspect_err(|error| {
            self.record_error(error.clone());
        })?;
        let bytes = std::fs::read(path).map_err(|e| format!("downloaded torrent object missing: {e}"))?;
        if let Ok(mut status) = self.status.lock() {
            status.last_fetch = Some(chrono::Utc::now().to_rfc3339());
            status.reachability = "direct".into();
            status.last_error = None;
        }
        Ok(bytes)
    }

    fn record_error(&self, error: String) {
        if let Ok(mut status) = self.status.lock() {
            status.reachability = "unreachable".into();
            status.last_error = Some(error);
        }
    }
}

fn validate_object_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid torrent object id".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use snartnet_core::validate_torrent_magnet_uri;

    #[test]
    fn rejects_path_traversal_object_ids() {
        assert!(validate_object_id("../secret").is_err());
        assert!(validate_object_id("message-123").is_ok());
    }

    #[test]
    fn published_magnets_are_valid_bit_torrent_v1_uris() {
        let root = tempfile::tempdir().unwrap();
        let node = TorrentNode::open(root.path(), 0).unwrap();
        let magnet = node.publish("profile-test", br"{}" ).unwrap();
        validate_torrent_magnet_uri(&magnet).unwrap();
        assert!(magnet.contains("dn=snartnet-profile-test"));
    }
}
