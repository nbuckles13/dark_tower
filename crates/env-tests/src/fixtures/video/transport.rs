//! The byte-shovelling seams under the video fixtures: [`UplinkTransport`] /
//! [`UplinkStream`] under [`super::VideoPublisher`], [`DownlinkStream`] under
//! [`super::VideoSubscriber`]. Implemented for wtransport here, and by
//! in-memory doubles in the self-tests, so every rule is tested with no QUIC.

#![expect(
    async_fn_in_trait,
    reason = "env-tests is single-runtime test code; callers never need a Send bound"
)]

use bytes::Bytes;

/// Why an uplink stream operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UplinkError {
    /// The peer sent STOP_SENDING with this code: MH closed the stream on its
    /// receive side (e.g. a stream-path attribution drop — `transport_mismatch`,
    /// `unmatched_stream` — or a stream-rate refusal). An OBSERVABLE outcome the
    /// consuming env-tests drive on purpose, never a fixture failure.
    Stopped(u32),
    /// Anything else (connection gone, protocol error): a fixture failure.
    Other(String),
}

impl std::fmt::Display for UplinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped(code) => write!(f, "stream stopped by the peer (code {code})"),
            Self::Other(e) => f.write_str(e),
        }
    }
}

/// One outgoing unidirectional stream.
pub trait UplinkStream {
    /// Write all of `bytes`.
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), UplinkError>;
    /// Finish the stream (FIN) explicitly. Awaits the peer's acknowledgement, so
    /// it too surfaces a STOP_SENDING.
    async fn finish(&mut self) -> Result<(), UplinkError>;
    /// Reset the stream with `code`.
    fn reset(&mut self, code: u32) -> Result<(), UplinkError>;
}

/// The publisher's connection to its media handler.
pub trait UplinkTransport {
    /// The stream type [`Self::open_uni`] yields.
    type Stream: UplinkStream;
    /// Open a unidirectional stream.
    async fn open_uni(&self) -> Result<Self::Stream, UplinkError>;
    /// Send one datagram.
    fn send_datagram(&self, bytes: Bytes) -> Result<(), UplinkError>;
    /// The largest datagram the peer accepts now (`None`: datagrams unsupported).
    fn max_datagram_size(&self) -> Option<usize>;
}

/// What one downlink read produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownlinkRead {
    /// `n` bytes were read into the buffer.
    Data(usize),
    /// FIN.
    Finished,
    /// The peer reset the stream.
    Reset,
    /// The connection failed (neither a FIN nor a stream reset).
    ConnectionLost,
}

/// One incoming unidirectional stream.
pub trait DownlinkStream {
    /// Read the next chunk into `buf`.
    async fn read_chunk(&mut self, buf: &mut [u8]) -> DownlinkRead;
}

fn write_error(e: wtransport::error::StreamWriteError) -> UplinkError {
    match e {
        wtransport::error::StreamWriteError::Stopped(code) => {
            UplinkError::Stopped(u32::try_from(code.into_inner()).unwrap_or(u32::MAX))
        }
        other => UplinkError::Other(other.to_string()),
    }
}

impl UplinkStream for wtransport::SendStream {
    async fn write_all(&mut self, bytes: &[u8]) -> Result<(), UplinkError> {
        wtransport::SendStream::write_all(self, bytes)
            .await
            .map_err(write_error)
    }
    async fn finish(&mut self) -> Result<(), UplinkError> {
        wtransport::SendStream::finish(self)
            .await
            .map_err(write_error)
    }
    fn reset(&mut self, code: u32) -> Result<(), UplinkError> {
        wtransport::SendStream::reset(self, wtransport::VarInt::from_u32(code))
            .map_err(|e| UplinkError::Other(e.to_string()))
    }
}

impl UplinkTransport for wtransport::Connection {
    type Stream = wtransport::SendStream;
    async fn open_uni(&self) -> Result<Self::Stream, UplinkError> {
        wtransport::Connection::open_uni(self)
            .await
            .map_err(|e| UplinkError::Other(e.to_string()))?
            .await
            .map_err(|e| UplinkError::Other(e.to_string()))
    }
    fn send_datagram(&self, bytes: Bytes) -> Result<(), UplinkError> {
        wtransport::Connection::send_datagram(self, bytes)
            .map_err(|e| UplinkError::Other(e.to_string()))
    }
    fn max_datagram_size(&self) -> Option<usize> {
        wtransport::Connection::max_datagram_size(self)
    }
}

impl DownlinkStream for wtransport::RecvStream {
    async fn read_chunk(&mut self, buf: &mut [u8]) -> DownlinkRead {
        match self.read(buf).await {
            Ok(Some(n)) => DownlinkRead::Data(n),
            Ok(None) => DownlinkRead::Finished,
            Err(wtransport::error::StreamReadError::Reset(_)) => DownlinkRead::Reset,
            Err(_) => DownlinkRead::ConnectionLost,
        }
    }
}

/// The in-memory doubles the self-tests drive the fixtures through.
#[cfg(test)]
pub(crate) mod memory {
    use super::{DownlinkRead, DownlinkStream, UplinkError, UplinkStream, UplinkTransport};
    use bytes::Bytes;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// What happened on the wire, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Wire {
        Opened(usize),
        Wrote(usize, Vec<u8>),
        Finished(usize),
        Reset(usize, u32),
        /// Dropped without finish or reset (a real `SendStream` drop is a FIN).
        Dropped(usize),
        Datagram(Vec<u8>),
    }

    /// Injected STOP_SENDING: stream `id` reports `Stopped(code)` on its
    /// `write`-th write (1-based), or on finish.
    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct StopPlan {
        pub(crate) on_write: Option<(usize, usize)>,
        pub(crate) on_finish: Option<usize>,
    }

    pub(crate) const STOP_CODE: u32 = 0x10;

    #[derive(Clone)]
    pub(crate) struct MemoryTransport {
        pub(crate) log: Arc<Mutex<Vec<Wire>>>,
        next: Arc<Mutex<usize>>,
        pub(crate) max_datagram: Option<usize>,
        pub(crate) stop: StopPlan,
    }

    impl MemoryTransport {
        pub(crate) fn new(max_datagram: Option<usize>) -> Self {
            Self {
                log: Arc::default(),
                next: Arc::default(),
                max_datagram,
                stop: StopPlan::default(),
            }
        }
        pub(crate) fn wire(&self) -> Vec<Wire> {
            self.log.lock().unwrap().clone()
        }
        /// Every byte written to stream `id`, concatenated.
        pub(crate) fn stream_bytes(&self, id: usize) -> Vec<u8> {
            self.wire()
                .into_iter()
                .filter_map(|w| match w {
                    Wire::Wrote(s, b) if s == id => Some(b),
                    _ => None,
                })
                .flatten()
                .collect()
        }
    }

    pub(crate) struct MemoryStream {
        id: usize,
        writes: usize,
        closed: bool,
        stop: StopPlan,
        log: Arc<Mutex<Vec<Wire>>>,
    }

    impl UplinkStream for MemoryStream {
        async fn write_all(&mut self, bytes: &[u8]) -> Result<(), UplinkError> {
            self.writes += 1;
            if self.stop.on_write == Some((self.id, self.writes)) {
                return Err(UplinkError::Stopped(STOP_CODE));
            }
            self.log
                .lock()
                .unwrap()
                .push(Wire::Wrote(self.id, bytes.to_vec()));
            Ok(())
        }
        async fn finish(&mut self) -> Result<(), UplinkError> {
            if self.stop.on_finish == Some(self.id) {
                return Err(UplinkError::Stopped(STOP_CODE));
            }
            self.closed = true;
            self.log.lock().unwrap().push(Wire::Finished(self.id));
            Ok(())
        }
        fn reset(&mut self, code: u32) -> Result<(), UplinkError> {
            self.closed = true;
            self.log.lock().unwrap().push(Wire::Reset(self.id, code));
            Ok(())
        }
    }

    impl Drop for MemoryStream {
        fn drop(&mut self) {
            if !self.closed {
                if let Ok(mut log) = self.log.lock() {
                    log.push(Wire::Dropped(self.id));
                }
            }
        }
    }

    impl UplinkTransport for MemoryTransport {
        type Stream = MemoryStream;
        async fn open_uni(&self) -> Result<MemoryStream, UplinkError> {
            let id = {
                let mut n = self.next.lock().unwrap();
                *n += 1;
                *n
            };
            self.log.lock().unwrap().push(Wire::Opened(id));
            Ok(MemoryStream {
                id,
                writes: 0,
                closed: false,
                stop: self.stop,
                log: self.log.clone(),
            })
        }
        fn send_datagram(&self, bytes: Bytes) -> Result<(), UplinkError> {
            self.log
                .lock()
                .unwrap()
                .push(Wire::Datagram(bytes.to_vec()));
            Ok(())
        }
        fn max_datagram_size(&self) -> Option<usize> {
            self.max_datagram
        }
    }

    /// A scripted downlink stream: each step is a chunk or a terminal outcome.
    pub(crate) struct ScriptedDownlink(pub(crate) VecDeque<Result<Vec<u8>, DownlinkRead>>);

    impl DownlinkStream for ScriptedDownlink {
        async fn read_chunk(&mut self, buf: &mut [u8]) -> DownlinkRead {
            match self.0.pop_front() {
                Some(Ok(chunk)) => {
                    buf[..chunk.len()].copy_from_slice(&chunk);
                    DownlinkRead::Data(chunk.len())
                }
                Some(Err(end)) => end,
                None => DownlinkRead::Finished,
            }
        }
    }
}
