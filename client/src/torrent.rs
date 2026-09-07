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
    sync::Arc,
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

const OBJECT_DIR: &str = "objects";
const DOWNLOAD_DIR: &str = "downloads";

pub struct TorrentNode {
    runtime: Arc<Runtime>,
    session: Arc<Session>,
    root: PathBuf,
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
        let options = SessionOptions {
            ipv4_only: true,
            dht: Some(DhtSessionConfig {
                // librqbit supplies a rotating default bootstrap list when this is None.
                bootstrap_addrs: None,
                port: Some(listen_port),
                persistence: None,
            }),
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpAndUtp,
                listen_addr: ([0, 0, 0, 0], listen_port).into(),
                announce_port: Some(listen_port),
                ipv4_only: true,
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
        })
    }

    /// Create a single-file torrent, seed it locally, and return its magnet.
    pub fn publish(&self, object_id: &str, bytes: &[u8]) -> Result<String, String> {
        validate_object_id(object_id)?;
        let path = self.root.join(OBJECT_DIR).join(format!("{object_id}.json"));
        std::fs::write(&path, bytes).map_err(|e| format!("torrent object write failed: {e}"))?;
        let torrent = self
            .runtime
            .block_on(async {
                let spawner = BlockingSpawner::new(2);
                create_torrent(&path, CreateTorrentOptions::default(), &spawner).await
            })
            .map_err(|e| format!("torrent metadata creation failed: {e:#}"))?;
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
            .map_err(|e| format!("torrent seed failed: {e:#}"))?;
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
            .map_err(|e| format!("torrent download failed: {e:#}"))?
            .into_handle()
            .ok_or("torrent was not added")?;
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(120), handle.wait_until_completed())
                .await
                .map_err(|_| "torrent download timed out".to_string())?
                .map_err(|e| format!("torrent completion failed: {e:#}"))
        })?;
        std::fs::read(path).map_err(|e| format!("downloaded torrent object missing: {e}"))
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

    #[test]
    fn rejects_path_traversal_object_ids() {
        assert!(validate_object_id("../secret").is_err());
        assert!(validate_object_id("message-123").is_ok());
    }
}
