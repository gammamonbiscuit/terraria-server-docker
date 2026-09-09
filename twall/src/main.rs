use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

// ------------------------- logging -------------------------

#[derive(Clone, Copy)]
enum Level {
    Info,
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Error => "ERROR",
        }
    }
}

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();

/// Open (or create) the log file in append mode. Never truncates.
/// Failing to open it aborts startup — better than running silently unlogged.
fn log_init() -> io::Result<()> {
    let path = std::env::var("proxy_log_file").unwrap_or_else(|_| "proxy.log".to_string());
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    if LOG_FILE.set(Mutex::new(file)).is_err() {
        return Err(io::Error::other("log file already initialized"));
    }
    Ok(())
}

/// Single entry point: writes the same line to console and file.
/// Format: [RFC 5424 timestamp] PROXY: [LEVEL] Message
fn log(level: Level, msg: impl AsRef<str>) {
    let line = format!(
        "[{}] PROXY: [{}] {}",
        rfc5424_timestamp(),
        level.tag(),
        msg.as_ref()
    );

    // Console: INFO -> stdout, ERROR -> stderr (same line text).
    match level {
        Level::Info => println!("{line}"),
        Level::Error => eprintln!("{line}"),
    }

    // File: one appended line per event. File is unbuffered, so the
    // write goes out immediately. A file failure must not kill the proxy.
    if let Some(file) = LOG_FILE.get() {
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "{line}");
        }
    }
}

fn info(msg: impl AsRef<str>) {
    log(Level::Info, msg);
}

fn error(msg: impl AsRef<str>) {
    log(Level::Error, msg);
}

/// Current time as an RFC 5424 timestamp: YYYY-MM-DDTHH:MM:SS.mmmZ (UTC).
fn rfc5424_timestamp() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();

    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let (y, mo, day) = civil_from_days((secs / 86_400) as i64);

    format!("{y:04}-{mo:02}-{day:02}T{h:02}:{m:02}:{s:02}.{:03}Z", d.subsec_millis())
}

/// Days since 1970-01-01 -> (year, month, day).
/// Howard Hinnant's `civil_from_days` algorithm — handles leap years
/// and is valid across the whole range we care about.
fn civil_from_days(days_since_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = yoe as i64 + era * 400 + u64::from(m <= 2) as i64;
    (y, m, d)
}

// ------------------------- proxy -------------------------

const LISTEN_ADDR: &str = "0.0.0.0:7778";
const BACKEND_ADDR: &str = "127.0.0.1:7777"; // Terraria default port

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Terraria packet framing: [len: u16 LE][msg_id: u8][payload...],
/// where `len` counts the header itself.
const HEADER_LEN: usize = 3;
const MSG_CONNECT_REQUEST: u8 = 0x01;

// Sane bounds for a Connect Request: header + length-prefixed version
// string. Real packets are ~15 bytes ("Terraria279"); 128 is generous.
const MIN_CONNECT_PKT: usize = 12;
const MAX_CONNECT_PKT: usize = 128;

const VERSION_TAG: &[u8] = b"Terraria";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    log_init()?;
    let listener = TcpListener::bind(LISTEN_ADDR).await?;
    info(format!("Listening on {LISTEN_ADDR}, backend {BACKEND_ADDR}"));

    loop {
        let (client, addr) = listener.accept().await?;
        info(format!("[{addr}] Client connected"));

        tokio::spawn(async move { handle_client(client, addr).await });
    }
}

/// Every path logs exactly one line and returns.
async fn handle_client(mut client: TcpStream, addr: SocketAddr) {
    let _ = client.set_nodelay(true);

    // --- Middleman: only Terraria's opening handshake gets through ---
    match timeout(HANDSHAKE_TIMEOUT, is_terraria_handshake(&client)).await {
        Ok(Ok(true)) => info(format!("[{addr}] Terraria handshake OK, forwarding")),
        Ok(Ok(false)) => {
            error(format!("[{addr}] Rejected: not Terraria traffic"));
            return;
        }
        Ok(Err(e)) => {
            error(format!("[{addr}] Handshake read failed: {e}"));
            return;
        }
        Err(_) => {
            error(format!("[{addr}] Rejected: handshake timeout"));
            return;
        }
    }

    // --- Plain proxying from here on ---
    let mut backend = match TcpStream::connect(BACKEND_ADDR).await {
        Ok(b) => b,
        Err(e) => {
            error(format!("[{addr}] Backend connect failed: {e}"));
            return;
        }
    };
    let _ = backend.set_nodelay(true);

    // Pump bytes both directions until either side closes.
    let (up, down) = match copy_bidirectional(&mut client, &mut backend).await {
        Ok(stats) => stats,
        Err(e) => {
            error(format!("[{addr}] Relay failed: {e}"));
            return;
        }
    };
    info(format!("[{addr}] Closed, {up} bytes up / {down} bytes down"));
}

/// Reads (via peek, so nothing is consumed) the client's first Terraria
/// packet and checks that it is a valid Connect Request.
async fn is_terraria_handshake(client: &TcpStream) -> io::Result<bool> {
    // Peek the 3-byte header.
    let mut header = [0u8; HEADER_LEN];
    peek_exact(client, &mut header).await?;

    let pkt_len = u16::from_le_bytes([header[0], header[1]]) as usize;
    let msg_id = header[2];

    if msg_id != MSG_CONNECT_REQUEST
        || !(MIN_CONNECT_PKT..=MAX_CONNECT_PKT).contains(&pkt_len)
    {
        return Ok(false);
    }

    // Peek the entire first packet and look for the version string.
    let mut pkt = vec![0u8; pkt_len];
    peek_exact(client, &mut pkt).await?;

    // The payload is a length-prefixed version string like "Terraria279".
    // Instead of parsing the exact string encoding, just search for the
    // ASCII bytes — robust and sufficient for identification.
    Ok(pkt[HEADER_LEN..]
        .windows(VERSION_TAG.len())
        .any(|w| w == VERSION_TAG))
}

/// Like `read_exact`, but via `peek` — the bytes stay in the socket
/// buffer and will be forwarded to the backend by copy_bidirectional.
async fn peek_exact(stream: &TcpStream, buf: &mut [u8]) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = stream.peek(&mut buf[filled..]).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "client closed before finishing handshake",
            ));
        }
        filled += n;
    }
    Ok(())
}