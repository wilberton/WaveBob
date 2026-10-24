//! Per-user GUI discovery and bounded local-socket request/reply transport.
//! No renderer dependencies: a GUI-free runner can attach to a desktop host.

use std::fs;
#[cfg(any(feature = "gui", test))]
use std::fs::OpenOptions;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(any(feature = "gui", test))]
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::thread;
#[cfg(any(feature = "gui", test))]
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use interprocess::local_socket::{GenericFilePath, GenericNamespaced, Name, Stream, prelude::*};
#[cfg(all(any(feature = "gui", test), not(windows)))]
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions};
use serde::{Deserialize, Serialize};

use crate::Response;

const MAX_FRAME: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstanceInfo {
    pub session_id: String,
    pub name: String,
    pub process_id: u32,
    pub protocol_version: u32,
}

#[derive(Clone, Deserialize, Serialize)]
struct Record {
    info: InstanceInfo,
    endpoint: String,
    namespaced: bool,
    token: String,
}

#[derive(Deserialize, Serialize)]
struct Packet {
    session_id: String,
    token: String,
    request: Option<String>,
    timeout_ms: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Reply {
    Pong { session_id: String },
    Response { response: Response },
    Failure { message: String },
}

#[cfg(any(feature = "gui", test))]
pub(crate) struct Incoming {
    pub line: String,
    pub deadline: Instant,
    pub reply: mpsc::Sender<Response>,
}

pub fn default_directory() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("WAVEBOB_RUNTIME_DIR") {
        return Ok(PathBuf::from(path));
    }
    let dirs = directories::ProjectDirs::from("org", "WaveBob", "WaveBob").ok_or_else(|| {
        io::Error::other("Cannot locate the current user's application directory")
    })?;
    Ok(dirs
        .runtime_dir()
        .unwrap_or(dirs.cache_dir())
        .join("instances"))
}

fn socket_name(endpoint: &str, namespaced: bool) -> io::Result<Name<'_>> {
    if namespaced {
        endpoint.to_ns_name::<GenericNamespaced>()
    } else {
        endpoint.to_fs_name::<GenericFilePath>()
    }
}

fn write_frame(stream: &mut Stream, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    let mut remaining = bytes;
    while !remaining.is_empty() {
        if Instant::now() >= deadline {
            #[cfg(test)]
            eprintln!(
                "[DEBUG-ipc21] write timeout total={} remaining={}",
                bytes.len(),
                remaining.len()
            );
            return Err(io::ErrorKind::TimedOut.into());
        }
        // Windows peers may use the backend's default 512-byte pipe buffers.
        // Unix sockets support bulk partial writes; tiny writes there exhaust
        // macOS socket buffers through per-write overhead and stall large schemas.
        let chunk = if cfg!(windows) { 512 } else { 64 * 1024 };
        #[cfg(unix)]
        stream.set_send_timeout(Some(
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_micros(1)),
        ))?;
        match stream.write(&remaining[..remaining.len().min(chunk)]) {
            // Windows byte-mode pipes may report a successful zero-byte write
            // when their nonblocking buffer is full. Retry under the deadline.
            Ok(0) if cfg!(windows) => thread::sleep(Duration::from_millis(2)),
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => remaining = &remaining[count..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_frame(stream: &mut Stream, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        if Instant::now() >= deadline {
            #[cfg(test)]
            eprintln!("[DEBUG-ipc21] read timeout received={}", frame.len());
            return Err(io::ErrorKind::TimedOut.into());
        }
        #[cfg(unix)]
        stream.set_recv_timeout(Some(
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_micros(1)),
        ))?;
        match stream.read(&mut chunk) {
            // Empty nonblocking Windows pipes can map ERROR_NO_DATA to Ok(0),
            // which is not distinguishable from EOF at this interface.
            Ok(0) if cfg!(windows) => thread::sleep(Duration::from_millis(2)),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                let end = chunk[..count].iter().position(|byte| *byte == b'\n');
                frame.extend_from_slice(&chunk[..end.unwrap_or(count)]);
                if frame.len() > MAX_FRAME {
                    return Err(io::Error::other("IPC frame exceeds 2 MiB"));
                }
                if end.is_some() {
                    return Ok(frame);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn exchange(record: &Record, request: Option<String>, timeout: Duration) -> io::Result<Reply> {
    let deadline = Instant::now() + timeout;
    let mut stream = Stream::connect(socket_name(&record.endpoint, record.namespaced)?)?;
    // Unix sockets can wait for readiness under a kernel-enforced deadline.
    // Windows named pipes retain the bounded nonblocking compatibility path.
    stream.set_nonblocking(cfg!(windows))?;
    let mut bytes = serde_json::to_vec(&Packet {
        session_id: record.info.session_id.clone(),
        token: record.token.clone(),
        request,
        timeout_ms: timeout.as_millis() as u64,
    })?;
    bytes.push(b'\n');
    write_frame(&mut stream, &bytes, deadline)?;
    Ok(serde_json::from_slice(&read_frame(&mut stream, deadline)?)?)
}

/// List only live, authenticated endpoints. Never chooses a session for callers.
pub fn discover(directory: &Path) -> io::Result<Vec<InstanceInfo>> {
    let files = match fs::read_dir(directory) {
        Ok(files) => files,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut instances = Vec::new();
    for file in files {
        let path = file?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
            continue;
        };
        if let Ok(Reply::Pong { session_id }) = exchange(&record, None, Duration::from_millis(250))
            && session_id == record.info.session_id
        {
            instances.push(record.info);
        }
    }
    instances.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok(instances)
}

pub struct Client {
    record: Record,
}

impl Client {
    pub fn attach(directory: &Path, session_id: &str) -> io::Result<Self> {
        let id = uuid::Uuid::parse_str(session_id)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Expected an explicit session UUID",
                )
            })?
            .to_string();
        let record: Record =
            serde_json::from_slice(&fs::read(directory.join(format!("{id}.json")))?)?;
        if record.info.session_id != id {
            return Err(io::Error::other("Instance identity mismatch"));
        }
        match exchange(&record, None, Duration::from_secs(2))? {
            Reply::Pong { session_id } if session_id == id => Ok(Self { record }),
            _ => Err(io::Error::other("Instance authentication failed")),
        }
    }

    pub fn call(&self, line: &str) -> io::Result<Response> {
        let timeout_ms = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|v| v.get("params")?.get("timeout_ms")?.as_u64())
            .unwrap_or(10_000)
            .clamp(1, 60_000);
        match exchange(
            &self.record,
            Some(line.into()),
            Duration::from_millis(timeout_ms + 2_000),
        )? {
            Reply::Response { response } if response.session_id == self.record.info.session_id => {
                Ok(response)
            }
            Reply::Failure { message } => Err(io::Error::other(message)),
            _ => Err(io::Error::other("Unexpected IPC response identity or type")),
        }
    }

    pub fn run(&self, reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
        for frame in reader.split(b'\n') {
            let frame = frame?;
            let line = std::str::from_utf8(&frame).unwrap_or("{");
            if line.trim().is_empty() {
                continue;
            }
            let response = self.call(line)?;
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        Ok(())
    }
}

#[cfg(any(feature = "gui", test))]
pub(crate) struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    record_path: PathBuf,
    #[cfg(feature = "gui")]
    active: Arc<AtomicUsize>,
}

#[cfg(any(feature = "gui", test))]
impl Server {
    #[cfg(feature = "gui")]
    pub fn is_idle(&self) -> bool {
        self.active.load(Ordering::Acquire) == 0
    }
    pub fn start(
        directory: &Path,
        session_id: &str,
        name: String,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<(Self, mpsc::Receiver<Incoming>)> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        let namespaced = GenericNamespaced::is_supported();
        let endpoint = if namespaced {
            format!("wavebob-{session_id}")
        } else {
            format!("/tmp/wavebob-{session_id}.sock")
        };
        #[cfg(not(windows))]
        let listener = ListenerOptions::new()
            .name(socket_name(&endpoint, namespaced)?)
            .nonblocking(ListenerNonblockingMode::Both)
            .create_sync()?;
        #[cfg(windows)]
        let listener = interprocess::os::windows::named_pipe::PipeListenerOptions::new()
            .path(format!(r"\\.\pipe\{endpoint}"))
            .nonblocking(true)
            .input_buffer_size_hint(64 * 1024)
            .output_buffer_size_hint(64 * 1024)
            .create_duplex::<interprocess::os::windows::named_pipe::pipe_mode::Bytes>()?;
        let record = Record {
            info: InstanceInfo {
                session_id: session_id.into(),
                name,
                process_id: std::process::id(),
                protocol_version: crate::PROTOCOL_VERSION,
            },
            endpoint,
            namespaced,
            token: uuid::Uuid::new_v4().to_string(),
        };
        let record_path = directory.join(format!("{session_id}.json"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&record_path)?;
        serde_json::to_writer(&mut file, &record)?;
        file.flush()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (sender, receiver) = mpsc::sync_channel(32);
        let wake = Arc::new(wake);
        let active = Arc::new(AtomicUsize::new(0));
        #[cfg(feature = "gui")]
        let server_active = active.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok(stream) => {
                        #[cfg(windows)]
                        let stream = Stream::from(
                            interprocess::os::windows::named_pipe::local_socket::Stream::from(
                                stream,
                            ),
                        );
                        if active.fetch_add(1, Ordering::AcqRel) >= 32 {
                            active.fetch_sub(1, Ordering::AcqRel);
                            continue;
                        }
                        let (record, sender, wake, active) =
                            (record.clone(), sender.clone(), wake.clone(), active.clone());
                        thread::spawn(move || {
                            let _ = serve(stream, &record, &sender, &*wake);
                            active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => {
                        eprintln!("IPC listener stopped: {error}");
                        break;
                    }
                }
            }
        });
        Ok((
            Self {
                stop,
                thread: Some(thread),
                record_path,
                #[cfg(feature = "gui")]
                active: server_active,
            },
            receiver,
        ))
    }
}

#[cfg(any(feature = "gui", test))]
fn serve(
    mut stream: Stream,
    record: &Record,
    sender: &mpsc::SyncSender<Incoming>,
    wake: &dyn Fn(),
) -> io::Result<()> {
    // Unix sockets can wait for readiness under a kernel-enforced deadline.
    // Windows named pipes retain the bounded nonblocking compatibility path.
    stream.set_nonblocking(cfg!(windows))?;
    let packet: Packet = serde_json::from_slice(&read_frame(
        &mut stream,
        Instant::now() + Duration::from_secs(2),
    )?)?;
    let deadline = Instant::now() + Duration::from_millis(packet.timeout_ms.clamp(1, 62_000));
    let reply = if packet.session_id != record.info.session_id || packet.token != record.token {
        Reply::Failure {
            message: "Instance authentication failed".into(),
        }
    } else if let Some(line) = packet.request {
        let (reply, response) = mpsc::channel();
        match sender.try_send(Incoming {
            line,
            deadline,
            reply,
        }) {
            Ok(()) => {
                wake();
                match response.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(response) => Reply::Response { response },
                    Err(_) => Reply::Failure {
                        message: "GUI request timed out or host closed".into(),
                    },
                }
            }
            Err(_) => Reply::Failure {
                message: "GUI request queue unavailable".into(),
            },
        }
    } else {
        Reply::Pong {
            session_id: record.info.session_id.clone(),
        }
    };
    let mut bytes = serde_json::to_vec(&reply)?;
    bytes.push(b'\n');
    write_frame(&mut stream, &bytes, deadline)
}

#[cfg(any(feature = "gui", test))]
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.record_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_endpoint_can_be_discovered_and_authenticated_without_a_renderer() {
        let directory = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let (_server, _incoming) =
            Server::start(directory.path(), &id, "probe".into(), || {}).unwrap();
        let client = Client::attach(directory.path(), &id);
        assert!(
            client.is_ok(),
            "Live endpoint handshake failed: {:?}",
            client.err()
        );
        let instances = discover(directory.path()).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].session_id, id);
    }

    #[test]
    fn schema_responses_larger_than_a_pipe_buffer_are_delivered() {
        let directory = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let (_server, incoming) =
            Server::start(directory.path(), &id, "large-reply".into(), || {}).unwrap();
        let client = Client::attach(directory.path(), &id).unwrap();
        let worker = thread::spawn(move || {
            let incoming = incoming.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(incoming.deadline > Instant::now());
            let started = Instant::now();
            eprintln!(
                "[DEBUG-ipc21] worker received, budget={:?}",
                incoming.deadline.saturating_duration_since(started)
            );
            let response =
                crate::protocol::dispatch_line(&mut crate::Session::default(), &incoming.line);
            let bytes = serde_json::to_vec(&response).unwrap().len();
            eprintln!(
                "[DEBUG-ipc21] worker serialized {bytes} bytes in {:?}",
                started.elapsed()
            );
            assert!(bytes > 64 * 1024);
            incoming.reply.send(response).unwrap();
        });
        let response = exchange(
            &client.record,
            Some(r#"{"id":"large","command":"session.capabilities"}"#.into()),
            Duration::from_millis(500),
        );
        assert!(
            matches!(response, Ok(Reply::Response { .. })),
            "Large response failed: {:?}",
            response.err()
        );
        worker.join().unwrap();
    }
}

#[cfg(all(test, unix))]
mod unix_deadline_tests {
    use super::*;
    #[test]
    fn stalled_unix_peer_cannot_block_frame_reads_or_writes_indefinitely() {
        let (stream, _unread_peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut stream = Stream::from(interprocess::os::unix::local_socket::Stream::from(stream));
        stream.set_nonblocking(false).unwrap();
        let start = Instant::now();
        let error = read_frame(&mut stream, start + Duration::from_millis(40)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
        let start = Instant::now();
        let error = write_frame(
            &mut stream,
            &vec![b'a'; MAX_FRAME],
            start + Duration::from_millis(40),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
