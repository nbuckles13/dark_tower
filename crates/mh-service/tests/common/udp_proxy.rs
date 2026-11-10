//! A loopback UDP relay with a blackhole switch, for close-linger tests.
//!
//! The client dials the relay; the relay forwards every datagram to the
//! upstream (the MH endpoint) and the replies back. With the switch ON, every
//! datagram in BOTH directions is dropped, so MH's peer is silent: it neither
//! acknowledges the session-close capsule nor reacts to it, and MH must end the
//! linger on its bound. Switching it OFF again lets MH's QUIC close (sent in
//! response to the client's next packet) reach the client.
//!
//! One client per relay (the first source address seen is the client).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

pub struct UdpProxy {
    /// The address the client dials.
    pub addr: SocketAddr,
    blackhole: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl UdpProxy {
    /// Relay `127.0.0.1:<ephemeral>` ⇄ `upstream`.
    pub async fn start(upstream: SocketAddr) -> Self {
        let front = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind relay front");
        let back = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind relay back");
        back.connect(upstream).await.expect("connect relay back");
        let addr = front.local_addr().expect("relay front addr");
        let blackhole = Arc::new(AtomicBool::new(false));
        let drop_all = Arc::clone(&blackhole);
        let task = tokio::spawn(async move {
            let mut client: Option<SocketAddr> = None;
            let mut up = vec![0u8; 65_536];
            let mut down = vec![0u8; 65_536];
            loop {
                tokio::select! {
                    r = front.recv_from(&mut up) => {
                        let Ok((n, from)) = r else { return };
                        client.get_or_insert(from);
                        if let (Some(bytes), false) = (up.get(..n), drop_all.load(Ordering::SeqCst)) {
                            let _ = back.send(bytes).await;
                        }
                    }
                    r = back.recv(&mut down) => {
                        let Ok(n) = r else { return };
                        if let (Some(to), Some(bytes), false) =
                            (client, down.get(..n), drop_all.load(Ordering::SeqCst))
                        {
                            let _ = front.send_to(bytes, to).await;
                        }
                    }
                }
            }
        });
        Self {
            addr,
            blackhole,
            task,
        }
    }

    /// The URL a WebTransport client dials to go through the relay.
    pub fn url(&self) -> String {
        format!("https://127.0.0.1:{}", self.addr.port())
    }

    /// Drop (`true`) or forward (`false`) every datagram, both directions.
    pub fn set_blackhole(&self, on: bool) {
        self.blackhole.store(on, Ordering::SeqCst);
    }
}

impl Drop for UdpProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}
