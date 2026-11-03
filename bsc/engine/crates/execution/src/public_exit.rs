//! REV.1.0 R5 (BSC3-030): the exit-only public route, locked by default
//! (`BSC_EXEC_LIVE_PUBLIC_EXIT=OFF`). With it ON, a LIVE exit step's signed
//! bytes (an approval, a SELL), once handed to the builders, also go to the
//! one configured JSON-RPC endpoint (`BSC_EXEC_LIVE_PUBLIC_EXIT_RPC`) as
//! `eth_sendRawTransaction`: the public mempool, where a SELL can be
//! sandwiched within its slippage (the operator's decision). Never a BUY.
//! Its own thread: the LIVE path never waits for it. JSON only on this
//! external wire.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use crate::chain_follower::{FollowerLimits, connect_bounds, http, https_endpoint};
use crate::wss::DisconnectReason;
use crate::wss::endpoint::{connect_tls, tls_config};

/// Where the public route's requests go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicTarget {
    /// The configured endpoint over TLS.
    Https(String),
    /// The stand-in on 127.0.0.1:`port` (the loopback route): nothing
    /// leaves the host.
    Loopback(u16),
}

/// What the endpoint made of one transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicOutcome {
    /// It answered the transaction's own hash.
    Accepted,
    /// A JSON-RPC error, or another answer.
    Rejected,
    /// No answer: connect, TLS, timeout, an HTTP status or framing.
    Failed,
}

/// The route's counts, kept by the LIVE path across its workers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PublicCounts {
    pub sent: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub failed: u64,
    /// Exit steps no worker took (none could be started, or it ended).
    pub unstarted: u64,
}

impl PublicCounts {
    /// Counts one reported outcome.
    pub fn count(&mut self, outcome: PublicOutcome) {
        match outcome {
            PublicOutcome::Accepted => self.accepted += 1,
            PublicOutcome::Rejected => self.rejected += 1,
            PublicOutcome::Failed => self.failed += 1,
        }
    }

    /// Each count as the status names it (with the providers' answers).
    pub fn named(&self) -> [(&'static str, u64); 5] {
        [
            ("PUBLIC_EXIT_SENT", self.sent),
            ("PUBLIC_EXIT_ACCEPTED", self.accepted),
            ("PUBLIC_EXIT_REJECTED", self.rejected),
            ("PUBLIC_EXIT_FAILED", self.failed),
            ("PUBLIC_EXIT_UNSTARTED", self.unstarted),
        ]
    }
}

/// The route's worker.
pub struct PublicExit {
    requests: Sender<([u8; 32], Vec<u8>)>,
    outcomes: Receiver<PublicOutcome>,
}

impl PublicExit {
    /// The worker for `target`, bounded by `limits` (the follower's
    /// connect, request and response bounds).
    pub fn spawn(target: &PublicTarget, limits: &FollowerLimits) -> Result<Self, String> {
        let (requests, inbox) = channel::<([u8; 32], Vec<u8>)>();
        let (report, outcomes) = channel();
        let limits = *limits;
        let worker = std::thread::Builder::new().name("bsc-public-exit".into());
        match target {
            PublicTarget::Https(url) => {
                // Never the endpoint in an error (it may hold a key).
                let endpoint = https_endpoint(url).map_err(|e| format!("{e:?}"))?;
                let tls = tls_config().map_err(|e| e.to_string())?;
                let bounds = connect_bounds(&limits);
                let (authority, path) = endpoint.authority_and_path();
                worker.spawn(move || {
                    let connect = |stop: &AtomicBool| {
                        connect_tls(&endpoint, &tls, &bounds, stop, Instant::now())
                    };
                    serve(
                        http::Client::new(connect, authority, path, &limits),
                        &inbox,
                        &report,
                    );
                })
            }
            PublicTarget::Loopback(port) => {
                let at = SocketAddr::from((Ipv4Addr::LOCALHOST, *port));
                worker.spawn(move || {
                    let connect = |_: &AtomicBool| {
                        let io = |e: std::io::Error| DisconnectReason::Io(e.kind());
                        let s =
                            TcpStream::connect_timeout(&at, limits.connect_timeout).map_err(io)?;
                        s.set_read_timeout(Some(limits.read_tick)).map_err(io)?;
                        s.set_write_timeout(Some(limits.write_timeout))
                            .map_err(io)?;
                        Ok(s)
                    };
                    serve(
                        http::Client::new(connect, at.to_string(), "/".into(), &limits),
                        &inbox,
                        &report,
                    );
                })
            }
        }
        .map_err(|e| e.to_string())?;
        Ok(Self { requests, outcomes })
    }

    /// Hands the transaction `tx_hash` (its signed bytes `raw`) to the
    /// worker; false once the worker ended.
    pub fn send(&self, tx_hash: [u8; 32], raw: &[u8]) -> bool {
        self.requests.send((tx_hash, raw.to_vec())).is_ok()
    }

    /// The outcomes the worker reported since the last call.
    pub fn poll(&self) -> Vec<PublicOutcome> {
        self.outcomes.try_iter().collect()
    }
}

/// Posts each transaction of `inbox` in turn and reports its outcome,
/// until either side ends.
fn serve<S, C>(
    mut client: http::Client<S, C>,
    inbox: &Receiver<([u8; 32], Vec<u8>)>,
    report: &Sender<PublicOutcome>,
) where
    S: Read + Write,
    C: FnMut(&AtomicBool) -> Result<S, DisconnectReason>,
{
    let stop = AtomicBool::new(false);
    for (tx_hash, raw) in inbox {
        let outcome = match client.post(&request(&raw), &stop) {
            Ok(body) => answer(&body, &tx_hash),
            Err(_) => PublicOutcome::Failed,
        };
        if report.send(outcome).is_err() {
            return;
        }
    }
}

/// The JSON-RPC request of the signed bytes `raw`.
pub fn request(raw: &[u8]) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":["0x{}"]}}"#,
        alloy_primitives::hex::encode(raw)
    )
    .into_bytes()
}

/// What the endpoint's answer `body` says about the transaction `tx_hash`
/// (a JSON-RPC error that is not `null` is a rejection).
pub fn answer(body: &[u8], tx_hash: &[u8; 32]) -> PublicOutcome {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return PublicOutcome::Rejected;
    };
    if value.get("error").is_some_and(|e| !e.is_null()) {
        return PublicOutcome::Rejected;
    }
    let own = format!("0x{}", alloy_primitives::hex::encode(tx_hash));
    match value.get("result").and_then(serde_json::Value::as_str) {
        Some(hash) if hash.eq_ignore_ascii_case(&own) => PublicOutcome::Accepted,
        _ => PublicOutcome::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The request names the method and the bytes; the answer is the
    /// transaction's own hash (also beside a `null` error), an error,
    /// another hash or no JSON.
    #[test]
    fn the_request_carries_the_bytes_and_only_the_own_hash_is_accepted() {
        assert_eq!(
            request(&[0x02, 0xab]),
            br#"{"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":["0x02ab"]}"#
        );
        let hash = [0xcd; 32];
        let own = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":"0x{}"}}"#,
            "CD".repeat(32)
        );
        assert_eq!(answer(own.as_bytes(), &hash), PublicOutcome::Accepted);
        let null = format!(
            r#"{{"jsonrpc":"2.0","id":1,"error":null,"result":"0x{}"}}"#,
            "cd".repeat(32)
        );
        assert_eq!(answer(null.as_bytes(), &hash), PublicOutcome::Accepted);
        let other = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":"0x{}"}}"#,
            "ce".repeat(32)
        );
        assert_eq!(answer(other.as_bytes(), &hash), PublicOutcome::Rejected);
        let error =
            br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"x"},"result":null}"#;
        assert_eq!(answer(error, &hash), PublicOutcome::Rejected);
        assert_eq!(answer(b"<html>", &hash), PublicOutcome::Rejected);
    }

    /// An endpoint that is not there: the step fails within the bounds and
    /// is counted, the LIVE path never waits.
    #[test]
    fn an_unreachable_endpoint_fails_within_its_bounds() {
        let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let limits = crate::runtime::exec::tests::limits();
        let worker = PublicExit::spawn(&PublicTarget::Loopback(port), &limits).unwrap();
        assert!(worker.send([1; 32], &[0x02]));
        let mut counts = PublicCounts::default();
        let deadline = Instant::now() + limits.request_timeout * 10;
        while counts.failed == 0 && Instant::now() < deadline {
            for o in worker.poll() {
                counts.count(o);
            }
            std::thread::sleep(limits.read_tick);
        }
        assert_eq!((counts.failed, counts.accepted, counts.rejected), (1, 0, 0));
    }
}
