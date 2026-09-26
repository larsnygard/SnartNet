//! One owner for the auxiliary sockets derived from the peer-facing bind (M7).
//!
//! The peer bind is the only port an operator configures. Everything else used to be
//! derived at each call site: `Session` opened a torrent node on `bind + 1` and a DHT node
//! on `bind + 2`, `TcpSwarmTransport` derived the same pair from its own base, `Session`
//! then fell back to the fixed `47472`/`47473` whenever a derivation failed, and every
//! failure was swallowed with `.ok()`. Two derivations and swallowed errors meant the
//! second binder silently ran without a node, so *who* owned the socket depended on start
//! order, and `bind + 1` (`47471` for the default bind) collided with LAN discovery.
//!
//! [`AuxiliaryPorts::resolve`] is now the single derivation point, and the layout is
//! fixed and documented:
//!
//! | socket | port | notes |
//! | --- | --- | --- |
//! | peer API / TCP sync | `base` | `SNARTNET_BIND`, default `47470` |
//! | LAN discovery (UDP) | `47471` | a LAN-wide constant, not derived |
//! | torrent (TCP + uTP) | `base + 3` | `47473` for the default bind |
//! | DHT (UDP) | `base + 4` | `47474` for the default bind |
//!
//! A derived port that is already taken moves to an OS-assigned port instead of a fixed
//! second choice, and an ephemeral base port (`:0`, used by tests and embedded hosts)
//! leaves both auxiliaries to the OS, so parallel sessions cannot collide.
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};

/// Torrent offset from the peer bind. Skips `47471` (LAN discovery).
const TORRENT_OFFSET: u16 = 3;

/// DHT offset from the peer bind.
const DHT_OFFSET: u16 = 4;

/// The auxiliary ports a peer bind owns. A `0` means "let the OS choose".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AuxiliaryPorts {
    /// Torrent listen port (TCP + uTP).
    pub torrent: u16,
    /// Mainline DHT port (UDP).
    pub dht: u16,
}

impl AuxiliaryPorts {
    /// Derive the auxiliary ports for a peer-facing bind address.
    ///
    /// An ephemeral bind (`:0`) returns `None`: that is the test and embedded-host mode,
    /// where a torrent session and a DHT node would add public network traffic that no
    /// peer ever dials. Every concrete bind owns both sockets.
    pub fn resolve(bind: SocketAddr) -> Option<Self> {
        let base = bind.port();
        if base == 0 {
            return None;
        }
        let torrent = match base.checked_add(TORRENT_OFFSET) {
            Some(port) if port_available(port, true) => port,
            _ => free_port(true),
        };
        let dht = match base.checked_add(DHT_OFFSET) {
            Some(port) if port != torrent && port_available(port, false) => port,
            _ => free_port(false),
        };
        Some(Self { torrent, dht })
    }
}

/// Whether a UDP (or TCP, when `tcp`) socket can still be bound on `port`.
fn port_available(port: u16, tcp: bool) -> bool {
    let udp_free = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok();
    if !tcp {
        return udp_free;
    }
    udp_free && TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

/// An OS-assigned free port of the right socket type.
fn free_port(tcp: bool) -> u16 {
    if tcp {
        return TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .and_then(|listener| listener.local_addr())
            .map(|addr| addr.port())
            .unwrap_or(0);
    }
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| socket.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_bind_owns_distinct_auxiliary_ports() {
        let ports = AuxiliaryPorts::resolve("127.0.0.1:47470".parse().unwrap())
            .expect("a concrete bind owns both auxiliaries");
        assert_eq!(ports.torrent, 47473);
        assert_eq!(ports.dht, 47474);
        // LAN discovery keeps 47471, so nothing else may derive it.
        assert_ne!(ports.torrent, 47471);
        assert_ne!(ports.dht, 47471);
        assert_ne!(ports.torrent, ports.dht);
    }

    #[test]
    fn an_ephemeral_bind_owns_no_auxiliary_sockets() {
        assert_eq!(
            AuxiliaryPorts::resolve("127.0.0.1:0".parse().unwrap()),
            None
        );
    }

    #[test]
    fn a_taken_derived_port_moves_to_the_os_instead_of_a_fixed_fallback() {
        let base = free_port(true);
        let taken = base.checked_add(TORRENT_OFFSET).unwrap();
        // Occupy the torrent port this bind would derive, on both socket types.
        let (Ok(_listener), Ok(_udp)) = (
            TcpListener::bind((Ipv4Addr::UNSPECIFIED, taken)),
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, taken)),
        ) else {
            // Someone else already owns the port: the fallback is untestable here.
            return;
        };
        let ports = AuxiliaryPorts::resolve(SocketAddr::from((Ipv4Addr::LOCALHOST, base)))
            .expect("a concrete bind owns both auxiliaries");
        assert_ne!(ports.torrent, taken);
        // 47472 is no longer a fixed "second choice" anywhere.
        assert_ne!(ports.torrent, 47472);
        assert_ne!(ports.dht, ports.torrent);
    }

    #[test]
    fn a_bind_near_the_port_ceiling_falls_back_to_the_os() {
        let ports = AuxiliaryPorts::resolve("127.0.0.1:65534".parse().unwrap())
            .expect("a concrete bind owns both auxiliaries");
        assert_ne!(ports.torrent, ports.dht);
        assert!(ports.torrent == 0 || ports.torrent > 1024);
    }
}
