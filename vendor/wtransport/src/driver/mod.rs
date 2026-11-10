use crate::datagram::Datagram;
use crate::driver::streams::biremote::StreamBiRemoteH3;
use crate::driver::streams::biremote::StreamBiRemoteWT;
use crate::driver::streams::session::StreamSession;
use crate::driver::streams::uniremote::StreamUniRemoteWT;
use crate::driver::streams::Stream;
use crate::driver::utils::bichannel;
use crate::driver::utils::shared_result;
use crate::driver::utils::SendError;
use crate::driver::utils::SharedResultGet;
use crate::driver::utils::SharedResultSet;
use crate::error::ApplicationClose;
use crate::error::SendDatagramError;
use crate::stream::OpeningBiStream;
use crate::stream::OpeningUniStream;
use crate::SessionId;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::debug_span;
use tracing::instrument;
use tracing::trace;
use tracing::Instrument;
use utils::BiChannelEndpoint;
use wtransport_proto::error::ErrorCode;
use wtransport_proto::frame::Frame;
use wtransport_proto::session::SessionRequest;
use wtransport_proto::settings::Settings;

#[derive(Clone, Debug)]
pub enum DriverError {
    Proto(ErrorCode),
    ApplicationClosed(ApplicationClose),
    NotConnected,
    /// PATCH(dark-tower ADR-0039 §7): this side sent CLOSE_WEBTRANSPORT_SESSION and the peer reacted on
    /// the CONNECT stream (or the connection ended). QUIC closes with H3_NO_ERROR.
    SessionCloseSent,
}

#[derive(Debug)]
pub struct Driver {
    quic_connection: quinn::Connection,
    ready_settings: Mutex<mpsc::Receiver<Settings>>,
    ready_sessions: BiChannelEndpoint<StreamSession>,
    ready_uni_wt_streams: Mutex<mpsc::Receiver<StreamUniRemoteWT>>,
    ready_bi_wt_streams: Mutex<mpsc::Receiver<StreamBiRemoteWT>>,
    ready_datagrams: Mutex<mpsc::Receiver<Datagram>>,
    driver_result: SharedResultGet<DriverError>,
    /// PATCH(dark-tower ADR-0039 §7): a second, dedicated subscriber to the driver result, so reading
    /// how the session ended never contends with the upstream `result()` waiters.
    session_end: SharedResultGet<DriverError>,
    /// PATCH(dark-tower ADR-0039 §7): hands an encoded CLOSE_WEBTRANSPORT_SESSION to the worker.
    close_session: mpsc::Sender<(Vec<u8>, tokio::sync::oneshot::Sender<Result<(), crate::error::SessionCloseError>>)>,
    /// PATCH(dark-tower ADR-0039 §7): the session close is sent at most once.
    close_session_started: std::sync::atomic::AtomicBool,
}

impl Driver {
    pub fn init(quic_connection: quinn::Connection) -> Self {
        let ready_settings = mpsc::channel(1);
        let ready_sessions = bichannel(1);
        let ready_uni_wt_streams = mpsc::channel(4);
        let ready_bi_wt_streams = mpsc::channel(1);
        let ready_datagrams = mpsc::channel(1);
        let driver_result = shared_result();
        // PATCH(dark-tower ADR-0039 §7)
        let session_end = driver_result.0.subscribe();
        let close_session = mpsc::channel(1);

        tokio::spawn(
            worker::Worker::new(
                quic_connection.clone(),
                ready_settings.0,
                ready_sessions.0,
                ready_uni_wt_streams.0,
                ready_bi_wt_streams.0,
                ready_datagrams.0,
                driver_result.0,
                close_session.1,
            )
            .run()
            .instrument(debug_span!("Driver", quic_id = quic_connection.stable_id())),
        );

        Self {
            quic_connection,
            ready_settings: Mutex::new(ready_settings.1),
            ready_sessions: ready_sessions.1,
            ready_uni_wt_streams: Mutex::new(ready_uni_wt_streams.1),
            ready_bi_wt_streams: Mutex::new(ready_bi_wt_streams.1),
            ready_datagrams: Mutex::new(ready_datagrams.1),
            driver_result: driver_result.1,
            session_end,
            close_session: close_session.0,
            close_session_started: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub async fn accept_settings(&self) -> Result<Settings, DriverError> {
        let mut lock = self.ready_settings.lock().await;

        match lock.recv().await {
            Some(settings) => Ok(settings),
            None => Err(self.result().await),
        }
    }

    pub async fn accept_session(&self) -> Result<StreamSession, DriverError> {
        match self.ready_sessions.recv().await {
            Some(session) => Ok(session),
            None => Err(self.result().await),
        }
    }

    pub async fn open_session(
        &self,
        session_request: SessionRequest,
    ) -> Result<StreamSession, DriverError> {
        let stream = Stream::open_bi(&self.quic_connection)
            .await
            .ok_or(DriverError::NotConnected)?
            .upgrade()
            .into_session(session_request);

        Ok(stream)
    }

    pub async fn register_session(&self, stream_session: StreamSession) -> Result<(), DriverError> {
        match self.ready_sessions.send(stream_session).await {
            Ok(()) => Ok(()),
            Err(SendError) => Err(self.result().await),
        }
    }

    pub async fn accept_uni(
        &self,
        session_id: SessionId,
    ) -> Result<StreamUniRemoteWT, DriverError> {
        let mut lock = self.ready_uni_wt_streams.lock().await;

        loop {
            let Some(stream) = lock.recv().await else {
                return Err(self.result().await);
            };

            if stream.session_id() == session_id {
                return Ok(stream);
            }

            debug!(
                "Discarding WT stream (stream_id: {}, session_id: {})",
                stream.id(),
                stream.session_id()
            );

            stream
                .into_stream()
                .stop(ErrorCode::BufferedStreamRejected.to_code())
                .expect("Stream not already stopped");
        }
    }

    pub async fn accept_bi(&self, session_id: SessionId) -> Result<StreamBiRemoteWT, DriverError> {
        let mut lock = self.ready_bi_wt_streams.lock().await;

        loop {
            let Some(stream) = lock.recv().await else {
                return Err(self.result().await);
            };

            if stream.session_id() == session_id {
                return Ok(stream);
            }

            debug!(
                "Discarding WT stream (stream_id: {}, session_id: {})",
                stream.id(),
                stream.session_id()
            );

            stream
                .into_stream()
                .1
                .stop(ErrorCode::BufferedStreamRejected.to_code())
                .expect("Stream not already stopped");
        }
    }

    pub async fn receive_datagram(&self, session_id: SessionId) -> Result<Datagram, DriverError> {
        let mut lock = self.ready_datagrams.lock().await;

        loop {
            let Some(datagram) = lock.recv().await else {
                return Err(self.result().await);
            };

            if datagram.session_id() == session_id {
                return Ok(datagram);
            }

            debug!(
                "Incoming datagram discarded (session_id: {})",
                datagram.session_id()
            );
        }
    }

    pub async fn open_uni(&self, session_id: SessionId) -> Result<OpeningUniStream, DriverError> {
        let quic_stream = Stream::open_uni(&self.quic_connection)
            .await
            .ok_or(DriverError::NotConnected)?;

        Ok(OpeningUniStream::new(session_id, quic_stream))
    }

    pub async fn open_bi(&self, session_id: SessionId) -> Result<OpeningBiStream, DriverError> {
        let quic_stream = Stream::open_bi(&self.quic_connection)
            .await
            .ok_or(DriverError::NotConnected)?;

        Ok(OpeningBiStream::new(session_id, quic_stream))
    }

    pub fn send_datagram(
        &self,
        session_id: SessionId,
        payload: &[u8],
    ) -> Result<(), SendDatagramError> {
        let quic_datagram = Datagram::write(session_id, payload).into_quic_bytes();

        match self.quic_connection.send_datagram(quic_datagram) {
            Ok(()) => Ok(()),
            Err(quinn::SendDatagramError::UnsupportedByPeer) => {
                Err(SendDatagramError::UnsupportedByPeer)
            }
            Err(quinn::SendDatagramError::Disabled) => {
                unreachable!()
            }

            Err(quinn::SendDatagramError::TooLarge) => Err(SendDatagramError::TooLarge),
            Err(quinn::SendDatagramError::ConnectionLost(_)) => {
                Err(SendDatagramError::NotConnected)
            }
        }
    }

    /// PATCH(dark-tower ADR-0039 §7): queues `capsule` (the exact bytes for the CONNECT stream) for the
    /// worker, which writes it verbatim and FINs the CONNECT stream's send half.
    ///
    /// Resolves once the bytes were written (the worker never blocks its loop
    /// on the peer's flow-control credit: the write is its own select arm). The
    /// CALLER bounds this wait. At most once per connection: every later call
    /// is `Err(AlreadyClosing)` and sends nothing.
    pub async fn close_session(
        &self,
        capsule: Vec<u8>,
    ) -> Result<(), crate::error::SessionCloseError> {
        use crate::error::SessionCloseError;
        if self
            .close_session_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(SessionCloseError::AlreadyClosing);
        }
        let (ack, acked) = tokio::sync::oneshot::channel();
        self.close_session
            .send((capsule, ack))
            .await
            .map_err(|_| SessionCloseError::DriverGone)?;
        acked.await.map_err(|_| SessionCloseError::DriverGone)?
    }

    /// PATCH(dark-tower ADR-0039 §7): how the driver ended, once it has ended (`None` if the worker
    /// vanished without a result).
    pub async fn session_end(&self) -> Option<DriverError> {
        self.session_end.result().await
    }

    async fn result(&self) -> DriverError {
        match self.driver_result.result().await {
            Some(error) => error,
            None => panic!("Driver worker panic!"),
        }
    }
}

mod worker {
    use super::*;
    use crate::driver::streams::qpack::RemoteQPackDecStream;
    use crate::driver::streams::qpack::RemoteQPackEncStream;
    use crate::driver::streams::settings::LocalSettingsStream;
    use crate::driver::streams::settings::RemoteSettingsStream;
    use crate::driver::streams::uniremote::StreamUniRemoteH3;
    use crate::driver::streams::ProtoReadError;
    use crate::driver::streams::ProtoWriteError;
    use crate::driver::utils::TrySendError;
    use streams::connect::ConnectStream;
    use utils::varint_w2q;
    use wtransport_proto::frame::FrameKind;

    /// PATCH(dark-tower ADR-0039 §7): this side's session close in progress.
    pub struct Closing {
        stream: StreamSession,
        capsule: Vec<u8>,
        written: usize,
        ack: Option<tokio::sync::oneshot::Sender<Result<(), crate::error::SessionCloseError>>>,
    }
    use wtransport_proto::headers::Headers;
    use wtransport_proto::session::HeadersParseError;
    use wtransport_proto::stream_header::StreamHeader;
    use wtransport_proto::stream_header::StreamKind;

    pub struct Worker {
        quic_connection: quinn::Connection,
        ready_settings: mpsc::Sender<Settings>,
        ready_sessions: BiChannelEndpoint<StreamSession>,
        ready_uni_wt_streams: mpsc::Sender<StreamUniRemoteWT>,
        ready_bi_wt_streams: mpsc::Sender<StreamBiRemoteWT>,
        ready_datagrams: mpsc::Sender<Datagram>,
        driver_result: SharedResultSet<DriverError>,
        local_settings_stream: LocalSettingsStream,
        remote_settings_stream: RemoteSettingsStream,
        remote_qpack_enc_stream: RemoteQPackEncStream,
        remote_qpack_dec_stream: RemoteQPackDecStream,
        connect_stream: ConnectStream,
        /// PATCH(dark-tower ADR-0039 §7): session-close requests from the `Driver`.
        close_session: mpsc::Receiver<(Vec<u8>, tokio::sync::oneshot::Sender<Result<(), crate::error::SessionCloseError>>)>,
        /// PATCH(dark-tower ADR-0039 §7): this side's session close in progress (capsule write, then
        /// waiting for the peer's reaction).
        closing: Option<Closing>,
    }

    impl Worker {
        pub fn new(
            quic_connection: quinn::Connection,
            ready_settings: mpsc::Sender<Settings>,
            ready_sessions: BiChannelEndpoint<StreamSession>,
            ready_uni_wt_streams: mpsc::Sender<StreamUniRemoteWT>,
            ready_bi_wt_streams: mpsc::Sender<StreamBiRemoteWT>,
            ready_datagrams: mpsc::Sender<Datagram>,
            driver_result: SharedResultSet<DriverError>,
            close_session: mpsc::Receiver<(Vec<u8>, tokio::sync::oneshot::Sender<Result<(), crate::error::SessionCloseError>>)>,
        ) -> Self {
            Self {
                quic_connection,
                ready_settings,
                ready_sessions,
                ready_uni_wt_streams,
                ready_bi_wt_streams,
                ready_datagrams,
                driver_result,
                local_settings_stream: LocalSettingsStream::empty(),
                remote_settings_stream: RemoteSettingsStream::empty(),
                remote_qpack_enc_stream: RemoteQPackEncStream::empty(),
                remote_qpack_dec_stream: RemoteQPackDecStream::empty(),
                connect_stream: ConnectStream::empty(),
                close_session,
                closing: None,
            }
        }

        pub async fn run(mut self) {
            debug!("Started");

            let error = self
                .run_impl()
                .await
                .expect_err("Worker must return an error");

            debug!("Ended with error: {:?}", error);

            match &error {
                DriverError::ApplicationClosed(_) => {
                    // Termination procedure
                    // TODO Reset send sides of all streams with SessionGone error
                    self.quic_connection
                        .close(varint_w2q(ErrorCode::NoError.to_code()), b"");
                }
                DriverError::Proto(error_code) => {
                    self.quic_connection
                        .close(varint_w2q(error_code.to_code()), b"");
                }
                DriverError::NotConnected => (),
                // PATCH(dark-tower ADR-0039 §7): the session close completed; never the app code on QUIC.
                DriverError::SessionCloseSent => {
                    self.quic_connection
                        .close(varint_w2q(ErrorCode::NoError.to_code()), b"");
                }
            }

            self.driver_result.set(error);
        }

        async fn run_impl(&mut self) -> Result<(), DriverError> {
            let mut remote_settings_watcher = self.remote_settings_stream.subscribe();
            let mut ready_uni_h3_streams = mpsc::channel(4);
            let mut ready_bi_h3_streams = mpsc::channel(1);

            self.open_and_send_settings().await?;

            loop {
                tokio::select! {
                    result = Self::accept_uni(&self.quic_connection,
                                              &ready_uni_h3_streams.0,
                                              &self.ready_uni_wt_streams) => {
                        result?;
                    }

                    result = Self::accept_bi(&self.quic_connection,
                                             &ready_bi_h3_streams.0,
                                             &self.ready_bi_wt_streams) => {
                        result?;
                    }

                    result = Self::accept_datagram(&self.quic_connection,
                                                   &self.ready_datagrams) => {
                        result?;
                    }

                    uni_h3_stream = ready_uni_h3_streams.1.recv() => {
                        let uni_h3_stream = uni_h3_stream.expect("Sender cannot be dropped")?;
                        self.handle_uni_h3_stream(uni_h3_stream)?;
                    }

                    bi_h3_stream = ready_bi_h3_streams.1.recv() => {
                        let (bi_h3_stream, first_frame) = bi_h3_stream.expect("Sender cannot be dropped")?;
                        self.handle_bi_h3_stream(bi_h3_stream, first_frame)?;
                    }


                    settings = remote_settings_watcher.accept_settings() => {
                        let settings = settings.expect("Channel cannot be dropped");
                        self.handle_remote_settings(settings)?;
                    }

                    stream_session = self.ready_sessions.recv() => {
                        match stream_session {
                            Some(stream_session) => {
                                if self.connect_stream.is_empty() {
                                    self.connect_stream.set_stream(stream_session);
                                }
                            }
                            None => return Err(DriverError::NotConnected),
                        };
                    }

                    error = Self::run_control_streams(&mut self.local_settings_stream,
                                                      &mut self.remote_settings_stream,
                                                      &mut self.remote_qpack_enc_stream,
                                                      &mut self.remote_qpack_dec_stream,
                                                      &mut self.connect_stream) => {
                        return Err(error);
                    }

                    // PATCH(dark-tower ADR-0039 §7): send this side's session close (once).
                    request = self.close_session.recv(), if self.closing.is_none() => {
                        match request {
                            Some((capsule, ack)) => self.start_session_close(capsule, ack),
                            None => return Err(DriverError::NotConnected),
                        }
                    }

                    // PATCH(dark-tower ADR-0039 §7): write the capsule (cancel-safe, its own arm, so a
                    // peer withholding flow-control credit never stalls this
                    // loop), FIN, then ANY peer read-side outcome on the CONNECT
                    // stream (FIN, reset, a frame, connection loss) completes
                    // the close with H3_NO_ERROR, never ClosedCriticalStream.
                    error = Self::drive_session_close(&mut self.closing), if self.closing.is_some() => {
                        return Err(error);
                    }

                    () = self.driver_result.closed() => {
                        return Err(DriverError::NotConnected);
                    }
                }
            }
        }

        /// PATCH(dark-tower ADR-0039 §7): takes the CONNECT stream for this side's session close. The
        /// stream leaves the upstream `ConnectStream`, so its capsule parser
        /// (and its reset → ClosedCriticalStream mapping) never runs on this
        /// side again. No await: nothing here waits on the peer.
        fn start_session_close(
            &mut self,
            capsule: Vec<u8>,
            ack: tokio::sync::oneshot::Sender<Result<(), crate::error::SessionCloseError>>,
        ) {
            match self.connect_stream.take_stream() {
                Some(stream) => {
                    self.closing = Some(Closing {
                        stream,
                        capsule,
                        written: 0,
                        ack: Some(ack),
                    });
                }
                None => {
                    let _ = ack.send(Err(crate::error::SessionCloseError::NoConnectStream));
                }
            }
        }

        /// PATCH(dark-tower ADR-0039 §7): drives this side's session close. Cancel-safe: progress lives
        /// in `Closing` (`written`, `ack`), and a single quinn `write` either
        /// completes or writes nothing when cancelled. Writes the capsule
        /// verbatim, FINs and acks the caller (once), then resolves on the
        /// peer's first read-side outcome. A write error is reported to the
        /// caller and also completes the close.
        async fn drive_session_close(closing: &mut Option<Closing>) -> DriverError {
            let Some(c) = closing.as_mut() else {
                return std::future::pending().await;
            };
            while c.ack.is_some() && c.written < c.capsule.len() {
                match c.stream.write_raw(&c.capsule[c.written..]).await {
                    Ok(n) => c.written += n,
                    Err(e) => {
                        if let Some(ack) = c.ack.take() {
                            let _ = ack.send(Err(crate::error::SessionCloseError::Write(e)));
                        }
                    }
                }
            }
            if let Some(ack) = c.ack.take() {
                c.stream.finish_nowait();
                let _ = ack.send(Ok(()));
            }
            let _ = c.stream.read_frame().await;
            DriverError::SessionCloseSent
        }

        async fn open_and_send_settings(&mut self) -> Result<(), DriverError> {
            assert!(self.local_settings_stream.is_empty());

            let stream = match Stream::open_uni(&self.quic_connection)
                .await
                .ok_or(DriverError::NotConnected)?
                .upgrade(StreamHeader::new_control())
                .await
            {
                Ok(h3_stream) => h3_stream,
                Err(ProtoWriteError::NotConnected) => return Err(DriverError::NotConnected),
                Err(ProtoWriteError::Stopped) => {
                    return Err(DriverError::Proto(ErrorCode::ClosedCriticalStream));
                }
            };

            self.local_settings_stream.set_stream(stream);
            self.local_settings_stream.send_settings().await
        }

        async fn accept_uni(
            quic_connection: &quinn::Connection,
            ready_uni_h3_streams: &mpsc::Sender<Result<StreamUniRemoteH3, DriverError>>,
            ready_uni_wt_streams: &mpsc::Sender<StreamUniRemoteWT>,
        ) -> Result<(), DriverError> {
            trace!("H3 uni queue capacity: {}", ready_uni_h3_streams.capacity());
            let h3_slot = ready_uni_h3_streams
                .clone()
                .reserve_owned()
                .await
                .expect("Receiver cannot be dropped");

            let wt_slot = match ready_uni_wt_streams.clone().reserve_owned().await {
                Ok(wt_slot) => wt_slot,
                Err(mpsc::error::SendError(_)) => return Err(DriverError::NotConnected),
            };

            let stream_quic = Stream::accept_uni(quic_connection)
                .await
                .ok_or(DriverError::NotConnected)?;

            let stream_id = stream_quic.id();
            debug!("New incoming uni stream ({})", stream_id);

            tokio::spawn(
                async move {
                    let stream_h3 = match stream_quic.upgrade().await {
                        Ok(stream_h3) => stream_h3,
                        Err(ProtoReadError::H3(error_code)) => {
                            h3_slot.send(Err(DriverError::Proto(error_code)));
                            return;
                        }
                        Err(ProtoReadError::IO(_)) => {
                            return;
                        }
                    };

                    let stream_kind = stream_h3.kind();
                    debug!("Type: {:?}", stream_kind);

                    if matches!(stream_kind, StreamKind::WebTransport) {
                        let stream_wt = stream_h3.upgrade();
                        wt_slot.send(stream_wt);
                    } else {
                        h3_slot.send(Ok(stream_h3));
                    }
                }
                .instrument(debug_span!("Stream", "id={}", stream_id)),
            );

            Ok(())
        }

        async fn accept_bi(
            quic_connection: &quinn::Connection,
            ready_bi_h3_streams: &mpsc::Sender<
                Result<(StreamBiRemoteH3, Frame<'static>), DriverError>,
            >,
            ready_bi_wt_streams: &mpsc::Sender<StreamBiRemoteWT>,
        ) -> Result<(), DriverError> {
            trace!("H3 bi queue capacity: {}", ready_bi_h3_streams.capacity());
            let h3_slot = ready_bi_h3_streams
                .clone()
                .reserve_owned()
                .await
                .expect("Receiver cannot be dropped");

            let wt_slot = match ready_bi_wt_streams.clone().reserve_owned().await {
                Ok(wt_slot) => wt_slot,
                Err(mpsc::error::SendError(_)) => return Err(DriverError::NotConnected),
            };

            let stream_quic = Stream::accept_bi(quic_connection)
                .await
                .ok_or(DriverError::NotConnected)?;

            let stream_id = stream_quic.id();
            debug!("New incoming bi stream ({})", stream_id);

            tokio::spawn(
                async move {
                    let mut stream_h3 = stream_quic.upgrade();

                    let frame = loop {
                        match stream_h3.read_frame().await {
                            Ok(frame) => {
                                debug!("Frame kind: {:?}", frame.kind());
                                if !matches!(frame.kind(), FrameKind::Exercise(_)) {
                                    break frame;
                                }
                            }
                            Err(ProtoReadError::H3(error_code)) => {
                                h3_slot.send(Err(DriverError::Proto(error_code)));
                                return;
                            }
                            Err(ProtoReadError::IO(_)) => {
                                return;
                            }
                        }
                    };

                    debug!("First frame: {:?}", frame);

                    match frame.session_id() {
                        Some(session_id) => {
                            let stream_wt = stream_h3.upgrade(session_id);
                            wt_slot.send(stream_wt);
                        }
                        None => {
                            h3_slot.send(Ok((stream_h3, frame)));
                        }
                    }
                }
                .instrument(debug_span!("Stream", "id={}", stream_id)),
            );

            Ok(())
        }

        async fn accept_datagram(
            quic_connection: &quinn::Connection,
            ready_datagrams: &mpsc::Sender<Datagram>,
        ) -> Result<(), DriverError> {
            let slot = match ready_datagrams.reserve().await {
                Ok(slot) => slot,
                Err(mpsc::error::SendError(_)) => return Err(DriverError::NotConnected),
            };

            let quic_dgram = quic_connection
                .read_datagram()
                .await
                .map_err(|_| DriverError::NotConnected)?;

            let datagram = match Datagram::read(quic_dgram) {
                Ok(datagram) => datagram,
                Err(error_code) => return Err(DriverError::Proto(error_code)),
            };

            trace!(
                "New incoming datagram (session_id: {})",
                datagram.session_id()
            );

            slot.send(datagram);

            Ok(())
        }

        fn handle_uni_h3_stream(&mut self, stream: StreamUniRemoteH3) -> Result<(), DriverError> {
            match stream.kind() {
                StreamKind::Control => {
                    if !self.remote_settings_stream.is_empty() {
                        return Err(DriverError::Proto(ErrorCode::StreamCreation));
                    }

                    self.remote_settings_stream.set_stream(stream);
                }
                StreamKind::QPackEncoder => {
                    if !self.remote_qpack_enc_stream.is_empty() {
                        return Err(DriverError::Proto(ErrorCode::StreamCreation));
                    }

                    self.remote_qpack_enc_stream.set_stream(stream);
                }
                StreamKind::QPackDecoder => {
                    if !self.remote_qpack_dec_stream.is_empty() {
                        return Err(DriverError::Proto(ErrorCode::StreamCreation));
                    }

                    self.remote_qpack_dec_stream.set_stream(stream);
                }
                StreamKind::WebTransport => unreachable!(),
                StreamKind::Exercise(_) => {}
            }

            Ok(())
        }

        #[instrument(skip_all, name = "Stream", fields(id = %stream.id()))]
        fn handle_bi_h3_stream(
            &mut self,
            mut stream: StreamBiRemoteH3,
            first_frame: Frame<'static>,
        ) -> Result<(), DriverError> {
            match first_frame.kind() {
                FrameKind::Data => {
                    return Err(DriverError::Proto(ErrorCode::FrameUnexpected));
                }
                FrameKind::Headers => {
                    let headers = match Headers::with_frame(&first_frame) {
                        Ok(headers) => headers,
                        Err(error_code) => return Err(DriverError::Proto(error_code)),
                    };

                    debug!("Headers: {:?}", headers);

                    let stream_session = match SessionRequest::try_from(headers) {
                        Ok(session_request) => stream.into_session(session_request),
                        Err(HeadersParseError::MethodNotConnect) => {
                            stream
                                .stop(ErrorCode::RequestRejected.to_code())
                                .expect("Stream not already stopped");
                            return Ok(());
                        }
                        // TODO(biagio): we might have more granularity with errors
                        Err(_) => {
                            stream
                                .stop(ErrorCode::Message.to_code())
                                .expect("Stream not already stopped");
                            return Ok(());
                        }
                    };

                    match self.ready_sessions.try_send(stream_session) {
                        Ok(()) => {}
                        Err(TrySendError::Full(mut stream)) => {
                            debug!("Discarding session request: sessions queue is full");
                            stream
                                .stop(ErrorCode::RequestRejected.to_code())
                                .expect("Stream not already stopped");
                        }
                        Err(TrySendError::Closed(_)) => return Err(DriverError::NotConnected),
                    }
                }
                FrameKind::Settings => {
                    return Err(DriverError::Proto(ErrorCode::FrameUnexpected));
                }
                FrameKind::WebTransport => unreachable!(),
                FrameKind::Exercise(_) => {}
            }

            Ok(())
        }

        async fn run_control_streams(
            local_settings: &mut LocalSettingsStream,
            remote_settings: &mut RemoteSettingsStream,
            remote_qpack_enc: &mut RemoteQPackEncStream,
            remote_qpack_dec: &mut RemoteQPackDecStream,
            connect_stream: &mut ConnectStream,
        ) -> DriverError {
            tokio::select! {
                error = connect_stream.run() => error,
                error = local_settings.run() => error,
                error = remote_settings.run() => error,
                error = remote_qpack_enc.run() => error,
                error = remote_qpack_dec.run() => error,
            }
        }

        fn handle_remote_settings(&mut self, settings: Settings) -> Result<(), DriverError> {
            debug!("Received: {:?}", settings);

            match self.ready_settings.try_send(settings) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Closed(_)) => Err(DriverError::NotConnected),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    unreachable!("No more than 1 setting frame can be processed")
                }
            }
        }
    }
}

pub(crate) mod streams;
pub(crate) mod utils;
