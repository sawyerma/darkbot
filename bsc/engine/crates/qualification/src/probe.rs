//! The safe application probe.
//!
//! An `eth_sendBundle` request with no transaction at all (`"txs":[]`) and
//! a last valid block of zero. It carries nothing that could execute or
//! move value, whatever the builder does with it. It exists to exercise a
//! real HTTP/1.1 and JSON-RPC round trip over the production transport;
//! the builder is expected to refuse it.
//!
//! The request is framed like the runner frames a bundle: the same request
//! line, the same headers in the same order, the `Host` header equal to the
//! TLS server name, and the authorization for BlockRazor.

use ring::digest;

/// The JSON-RPC body of the probe with request id `id`: no transaction,
/// `maxBlockNumber` zero, `noMerge` and `positionFirst` as the birth-entry
/// envelope sends them.
pub fn probe_body(id: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":\"{id}\",\"method\":\"eth_sendBundle\",\
         \"params\":[{{\"txs\":[],\"maxBlockNumber\":0,\"noMerge\":true,\"positionFirst\":true}}]}}"
    )
}

/// The complete HTTP/1.1 request for `body`, to `host`, with the
/// authorization if there is one. It may hold a secret; it is never
/// written into evidence or output, and never hashed.
pub fn frame_request(host: &str, authorization: Option<&str>, body: &str) -> Vec<u8> {
    let length = body.len().to_string();
    let mut request = Vec::with_capacity(body.len() + host.len() + 160);
    request.extend_from_slice(b"POST / HTTP/1.1\r\nHost: ");
    request.extend_from_slice(host.as_bytes());
    request.extend_from_slice(b"\r\nContent-Type: application/json\r\nContent-Length: ");
    request.extend_from_slice(length.as_bytes());
    request.extend_from_slice(b"\r\n");
    if let Some(value) = authorization {
        request.extend_from_slice(b"Authorization: ");
        request.extend_from_slice(value.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    request.extend_from_slice(b"\r\n");
    request.extend_from_slice(body.as_bytes());
    request
}

/// SHA-256 of `bytes`, as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let hash = digest::digest(&digest::SHA256, bytes);
    hash.as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
