use std::io;
use std::net::SocketAddr;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

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
    let listener = TcpListener::bind(LISTEN_ADDR).await?;
    println!("[twall] Terraria proxy: {LISTEN_ADDR} -> {BACKEND_ADDR}");

    loop {
        let (client, addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_client(client, addr).await {
                eprintln!("[twall] [{addr}] error: {e}");
            }
        });
    }
}

async fn handle_client(mut client: TcpStream, addr: SocketAddr) -> io::Result<()> {
    let _ = client.set_nodelay(true);

    // --- Middleman: only Terraria's opening handshake gets through ---
    match timeout(HANDSHAKE_TIMEOUT, is_terraria_handshake(&client)).await {
        Ok(Ok(true)) => println!("[twall] [{addr}] Terraria handshake OK, forwarding"),
        Ok(Ok(false)) => {
            eprintln!("[twall] [{addr}] not Terraria traffic, closing");
            return Ok(()); // drop -> closed, backend never contacted
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            eprintln!("[twall] [{addr}] handshake timeout, closing");
            return Ok(());
        }
    }

    // --- Plain proxying from here on ---
    let mut backend = TcpStream::connect(BACKEND_ADDR).await?;
    let _ = backend.set_nodelay(true);

    let (up, down) = copy_bidirectional(&mut client, &mut backend).await?;
    println!("[twall] [{addr}] relayed {up} up / {down} down bytes");
    Ok(())
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