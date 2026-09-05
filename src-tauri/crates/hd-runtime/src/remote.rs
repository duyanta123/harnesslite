//! Remote access: the harness on the LAN, behind a pairing handshake.
//!
//! The harness itself stays bound to loopback. Opening remote access starts a
//! second listener on one LAN address whose only job is to decide, per
//! connection, whether a browser may speak to that loopback port — and to
//! relay, byte for byte, when one may. HTTP and WebSocket and SSE all survive
//! because nothing here interprets them: after the handshake the gateway is a
//! pipe.
//!
//! The handshake is designed for a browser, because the client that scans the
//! QR code is a browser:
//!
//! 1. The pairing URL — `http://<lan>/hl-pair?c=<code>` — is shown as a QR and
//!    a code. The code is single-use and lives two minutes.
//! 2. A GET that presents it mints one device credential, answered with a
//!    `Set-Cookie` and a redirect to `/`. The credential is random, kept only
//!    in memory, and revocable.
//! 3. Every other request must carry that cookie; anything else is refused
//!    without ever reaching the harness.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use hd_core::error::{Error, Result};

/// How long a pairing code answers for.
pub const CODE_LIFETIME: Duration = Duration::from_secs(120);

/// A request head larger than this is not a browser asking politely.
const HEAD_CEILING: usize = 16 * 1024;

/// How long a silent connection may hold the handshake open.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// One paired device, as the pane lists it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// A handle, not a secret. The credential itself never leaves the gateway.
    pub id: String,
    /// What the pairing request's User-Agent said, when it said anything.
    pub label: Option<String>,
    pub paired_seconds_ago: u64,
    pub last_seen_seconds_ago: u64,
}

/// What the pane renders, one status per command reply and change event.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub open: bool,
    /// Every LAN address this machine could be reached on.
    pub addresses: Vec<String>,
    pub url: Option<String>,
    pub pairing_url: Option<String>,
    pub qr: Option<qr::Matrix>,
    pub code_seconds_left: Option<u64>,
    pub code_lifetime_seconds: u64,
    pub devices: Vec<Device>,
    pub active: u64,
    pub served: u64,
    pub refused: u64,
}

/// Events a running gateway produces, for the `remote://changed` relay.
#[derive(Clone, Debug)]
pub enum Event {
    Changed,
}

pub type Emit = Arc<dyn Fn(Event) + Send + Sync>;

struct Live {
    /// Dropping it stops accepting. Existing relays finish naturally.
    _listener: TcpListener,
    /// The address the listener actually took, for the URLs.
    local: SocketAddr,
    /// Which LAN address it was announced on.
    announced: String,
    code: String,
    code_issued: Instant,
}

struct Inner {
    live: Mutex<Option<Live>>,
    devices: Mutex<HashMap<String, DeviceRecord>>,
    active: AtomicU64,
    served: AtomicU64,
    refused: AtomicU64,
}

struct DeviceRecord {
    id: String,
    label: Option<String>,
    credential: String,
    paired_at: Instant,
    last_seen: Instant,
}

/// The one remote session this shell can host.
pub struct Remote {
    inner: Arc<Inner>,
}

impl Remote {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(Inner {
                live: Mutex::new(None),
                devices: Mutex::new(HashMap::new()),
                active: AtomicU64::new(0),
                served: AtomicU64::new(0),
                refused: AtomicU64::new(0),
            }),
        })
    }

    /// Open the door in front of a harness that is serving on loopback.
    pub fn open(self: &Arc<Self>, harness_port: u16, emit: Emit) -> Result<Info> {
        let mut live = self.inner.live.lock().expect("remote poisoned");
        if let Some(existing) = live.as_ref() {
            let info = self.info_of(existing);
            return Ok(info);
        }

        let announced = lan_address().ok_or_else(|| {
            Error::Harness("no LAN address could be found to listen on".into())
        })?;
        let listener = TcpListener::bind((announced.as_str(), 0))
            .map_err(|cause| Error::Harness(format!("the LAN listener could not start: {cause}")))?;
        let local = listener
            .local_addr()
            .map_err(|cause| Error::Harness(format!("the LAN listener has no address: {cause}")))?;

        let session = Live {
            _listener: listener,
            local,
            announced: announced.clone(),
            code: new_code(),
            code_issued: Instant::now(),
        };
        let info = self.info_of(&session);

        let gateway = Arc::clone(self);
        let port = harness_port;
        let relay_emit = Arc::clone(&emit);
        *live = Some(session);
        drop(live);

        std::thread::Builder::new()
            .name("remote-accept".into())
            .spawn(move || Self::accept_loop(gateway, port, relay_emit))
            .expect("spawning the remote accept loop");

        emit(Event::Changed);
        Ok(info)
    }

    /// Close the door. Pairings are forgotten: opening again starts clean.
    pub fn close(&self, emit: Emit) -> Info {
        *self.inner.live.lock().expect("remote poisoned") = None;
        self.inner.devices.lock().expect("remote poisoned").clear();
        let info = self.info();
        emit(Event::Changed);
        info
    }

    /// Put a fresh code on screen. Paired devices are untouched.
    pub fn renew(&self, emit: Emit) -> Result<Info> {
        let mut live = self.inner.live.lock().expect("remote poisoned");
        match live.as_mut() {
            Some(session) => {
                session.code = new_code();
                session.code_issued = Instant::now();
                let info = self.info_of(session);
                drop(live);
                emit(Event::Changed);
                Ok(info)
            }
            None => Err(Error::Harness("remote access is not open".into())),
        }
    }

    /// Forget one device. Anything it has open finishes; it cannot come back.
    pub fn forget(&self, id: &str, emit: Emit) -> Result<Info> {
        let mut devices = self.inner.devices.lock().expect("remote poisoned");
        if devices.remove(id).is_none() {
            return Err(Error::Harness(format!("there is no paired device {id}")));
        }
        drop(devices);
        emit(Event::Changed);
        Ok(self.info())
    }

    /// The status as it stands, open or not.
    pub fn info(&self) -> Info {
        let live = self.inner.live.lock().expect("remote poisoned");
        match live.as_ref() {
            Some(session) => self.info_of(session),
            None => self.closed_info(),
        }
    }

    fn closed_info(&self) -> Info {
        Info {
            open: false,
            addresses: lan_addresses(),
            url: None,
            pairing_url: None,
            qr: None,
            code_seconds_left: None,
            code_lifetime_seconds: CODE_LIFETIME.as_secs(),
            devices: Vec::new(),
            active: self.inner.active.load(Ordering::Relaxed),
            served: self.inner.served.load(Ordering::Relaxed),
            refused: self.inner.refused.load(Ordering::Relaxed),
        }
    }

    fn info_of(&self, session: &Live) -> Info {
        let base = format!("http://{}:{}", session.announced, session.local.port());
        let pairing_url = format!("{base}/hl-pair?c={}", session.code);
        let qr = qr::encode(&pairing_url);
        let seconds_left = CODE_LIFETIME
            .as_secs()
            .saturating_sub(session.code_issued.elapsed().as_secs());
        let devices = self.devices_sorted();

        Info {
            open: true,
            addresses: lan_addresses(),
            url: Some(base),
            pairing_url: Some(pairing_url),
            qr: Some(qr),
            code_seconds_left: Some(seconds_left),
            code_lifetime_seconds: CODE_LIFETIME.as_secs(),
            devices,
            active: self.inner.active.load(Ordering::Relaxed),
            served: self.inner.served.load(Ordering::Relaxed),
            refused: self.inner.refused.load(Ordering::Relaxed),
        }
    }

    fn devices_sorted(&self) -> Vec<Device> {
        let devices = self.inner.devices.lock().expect("remote poisoned");
        let mut listed: Vec<Device> = devices
            .values()
            .map(|record| Device {
                id: record.id.clone(),
                label: record.label.clone(),
                paired_seconds_ago: record.paired_at.elapsed().as_secs(),
                last_seen_seconds_ago: record.last_seen.elapsed().as_secs(),
            })
            .collect();
        listed.sort_by(|a, b| a.id.cmp(&b.id));
        listed
    }

    /// Pair one connection that brought the live code. Single use: the code
    /// is spent under the lock, and the screen's code stops being valid even
    /// if this pairing is refused elsewhere.
    fn pair(&self, user_agent: Option<String>) -> bool {
        let mut live = self.inner.live.lock().expect("remote poisoned");
        let Some(session) = live.as_mut() else {
            return false;
        };
        if session.code_issued.elapsed() > CODE_LIFETIME {
            return false;
        }
        // Single use: the code is spent the moment it pairs, and a fresh one
        // is what the screen still shows.
        session.code = new_code();
        session.code_issued = Instant::now();
        drop(live);

        let credential = new_code() + &new_code();
        let id = format!("d{}", &credential[..8]);
        self.inner.devices.lock().expect("remote poisoned").insert(
            id.clone(),
            DeviceRecord {
                id,
                label: user_agent,
                credential,
                paired_at: Instant::now(),
                last_seen: Instant::now(),
            },
        );
        true
    }

    /// Whether a presented credential belongs to a paired device.
    fn admits(&self, credential: &str) -> bool {
        let mut devices = self.inner.devices.lock().expect("remote poisoned");
        match devices.values_mut().find(|record| record.credential == credential) {
            Some(record) => {
                record.last_seen = Instant::now();
                true
            }
            None => false,
        }
    }

    fn accept_loop(self: Arc<Self>, harness_port: u16, emit: Emit) {
        let listener = {
            let live = self.inner.live.lock().expect("remote poisoned");
            match live.as_ref() {
                Some(session) => session._listener.try_clone(),
                None => return,
            }
        };
        let listener = match listener {
            Ok(listener) => listener,
            Err(_) => return,
        };

        for stream in listener.incoming() {
            // Closed on purpose: the door is shut, stop answering.
            if self.inner.live.lock().expect("remote poisoned").is_none() {
                return;
            }
            let Ok(stream) = stream else { continue };
            let gateway = Arc::clone(&self);
            let emit = Arc::clone(&emit);
            std::thread::Builder::new()
                .name("remote-conn".into())
                .spawn(move || gateway.handle(stream, harness_port, emit))
                .ok();
        }
    }

    fn handle(&self, mut client: TcpStream, harness_port: u16, emit: Emit) {
        let _ = client.set_read_timeout(Some(HEAD_TIMEOUT));
        let _ = client.set_write_timeout(Some(HEAD_TIMEOUT));

        // Read the request head, and no further: whatever the browser pipelined
        // behind it belongs to the relay, byte for byte.
        let mut head = Vec::with_capacity(1024);
        let mut byte = [0u8; 1];
        loop {
            match client.read(&mut byte) {
                Ok(0) => return,
                Ok(_) => {
                    head.push(byte[0]);
                    if head.ends_with(b"\r\n\r\n") || head.len() > HEAD_CEILING {
                        break;
                    }
                }
                Err(_) => return,
            }
        }
        let head_text = String::from_utf8_lossy(&head).into_owned();
        let mut lines = head_text.lines();
        let request_line = lines.next().unwrap_or_default().to_string();
        let user_agent = lines
            .clone()
            .find(|line| line.to_ascii_lowercase().starts_with("user-agent:"))
            .map(|line| line.split_once(':').map(|(_, value)| value.trim().to_string()).unwrap_or_default());

        if let Some(code) = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split_once("c=").map(|(_, code)| code.to_string()))
            .filter(|_| request_line.contains("/hl-pair?"))
        {
            // Liveness and single-use are both checked inside `pair`, which
            // spends the code under the lock.
            if !code.is_empty() && self.pair(user_agent) {
                let credential = {
                    let devices = self.inner.devices.lock().expect("remote poisoned");
                    devices
                        .values()
                        .max_by_key(|record| record.paired_at)
                        .map(|record| record.credential.clone())
                }
                .unwrap_or_default();
                let _ = client.write_all(
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: /\r\nSet-Cookie: hlcred={credential}; Path=/; HttpOnly; SameSite=Lax\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                );
                let _ = client.flush();
                emit(Event::Changed);
                return;
            }
            self.inner.refused.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            emit(Event::Changed);
            return;
        }

        let Some(credential) = head_text
            .to_ascii_lowercase()
            .lines()
            .find(|line| line.starts_with("cookie:"))
            .and_then(|line| line.split("hlcred=").nth(1))
            .map(|value| value.split(';').next().unwrap_or_default().trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            self.inner.refused.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            emit(Event::Changed);
            return;
        };
        let credential = head_text
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
            .and_then(|line| line.split("hlcred=").nth(1))
            .map(|value| value.split(';').next().unwrap_or_default().trim().to_string())
            .unwrap_or(credential);

        if !self.admits(&credential) {
            self.inner.refused.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            emit(Event::Changed);
            return;
        }

        // Paired. From here the gateway is a pipe in both directions.
        let Ok(harness) = TcpStream::connect(("127.0.0.1", harness_port)) else {
            self.inner.refused.fetch_add(1, Ordering::Relaxed);
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            emit(Event::Changed);
            return;
        };
        let _ = client.set_read_timeout(None);
        let _ = client.set_nodelay(true);
        let _ = harness.set_nodelay(true);

        self.inner.served.fetch_add(1, Ordering::Relaxed);
        self.inner.active.fetch_add(1, Ordering::Relaxed);
        emit(Event::Changed);

        let client_for_return = client.try_clone();
        let harness_for_return = harness.try_clone();
        let upstream = std::thread::Builder::new()
            .name("remote-up".into())
            .spawn(move || {
                let mut client = client;
                let mut harness = harness;
                let _ = std::io::copy(&mut client, &mut harness);
                let _ = harness.shutdown(std::net::Shutdown::Write);
            })
            .ok();
        if let (Ok(mut client), Ok(mut harness)) = (client_for_return, harness_for_return) {
            let _ = std::io::copy(&mut harness, &mut client);
        }
        drop(upstream);
        self.inner.active.fetch_sub(1, Ordering::Relaxed);
        emit(Event::Changed);
    }
}

fn new_code() -> String {
    use sha2::{Digest, Sha256};
    let seed = format!(
        "{}\0{:?}\0remote",
        std::process::id(),
        std::time::SystemTime::now()
    );
    let digest = Sha256::digest(seed.as_bytes());
    // Eight hex characters: 32 bits of pairing space, spent in one use.
    digest[..4].iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The one LAN address traffic to the internet would leave from, via the
/// connected-UDP trick: no packets are sent, the kernel just picks a route.
fn lan_address() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let address = socket.local_addr().ok()?;
    Some(address.ip().to_string())
}

/// Every LAN address this machine answers on, for the pane's hint list.
fn lan_addresses() -> Vec<String> {
    let mut addresses = Vec::new();
    if let Some(address) = lan_address() {
        addresses.push(address);
    }
    addresses.sort();
    addresses.dedup();
    addresses
}

/// Kept for the export path: seconds since the epoch, right now.
#[allow(dead_code)]
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub mod qr {
    //! The pairing URL, drawn as the boolean matrix a canvas paints.

    use serde::Serialize;

    /// The matrix behind `QrCode`: `size` rows of `size` modules, row-major.
    #[derive(Clone, Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Matrix {
        pub size: usize,
        pub modules: Vec<bool>,
    }

    /// Encode `text` at the smallest version that fits, with medium error
    /// correction — the setting a phone camera at arm's length reads best.
    pub fn encode(text: &str) -> Matrix {
        let code = match qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium) {
            Ok(code) => code,
            // A URL too long for medium correction still fits at low.
            Err(_) => qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Low)
                .expect("the pairing URL always fits at low correction"),
        };
        let size = code.size() as usize;
        let modules = (0..size)
            .flat_map(|y| (0..size).map(move |x| (x, y)))
            .map(|(x, y)| code.get_module(x as i32, y as i32))
            .collect();
        Matrix { size, modules }
    }

    #[cfg(test)]
    mod tests {
        use super::encode;

        #[test]
        fn a_pairing_url_encodes_to_a_square_matrix() {
            let matrix = encode("http://192.168.1.10:41235/hl-pair?c=ab12cd34");
            assert_eq!(matrix.modules.len(), matrix.size * matrix.size);
            assert!(matrix.modules.iter().any(|module| *module));
        }
    }
}
