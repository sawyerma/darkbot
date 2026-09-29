use alloy_primitives::Address;
use serde_json::{json, Value};
use std::{
    fs,
    hint::black_box,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    str::FromStr,
    thread,
    time::Instant,
};

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }
    if b.len() <= 55 {
        let mut out = Vec::with_capacity(1 + b.len());
        out.push(0x80 + b.len() as u8);
        out.extend_from_slice(b);
        return out;
    }
    let len_bytes = b.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];
    let mut out = Vec::with_capacity(1 + lb.len() + b.len());
    out.push(0xb7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(b);
    out
}

fn rlp_hex(s: &str) -> Vec<u8> {
    let mut b = hex::decode(s.trim_start_matches("0x")).unwrap();
    while b.first() == Some(&0) {
        b.remove(0);
    }
    rlp_bytes(&b)
}

fn rlp_list(items: Vec<Vec<u8>>) -> Vec<u8> {
    let payload_len: usize = items.iter().map(Vec::len).sum();
    let mut payload = Vec::with_capacity(payload_len);

    for item in items {
        payload.extend_from_slice(&item);
    }

    if payload.len() <= 55 {
        let mut out = Vec::with_capacity(1 + payload.len());
        out.push(0xc0 + payload.len() as u8);
        out.extend_from_slice(&payload);
        return out;
    }

    let len_bytes = payload.len().to_be_bytes();
    let first = len_bytes.iter().position(|x| *x != 0).unwrap();
    let lb = &len_bytes[first..];

    let mut out = Vec::with_capacity(1 + lb.len() + payload.len());
    out.push(0xf7 + lb.len() as u8);
    out.extend_from_slice(lb);
    out.extend_from_slice(&payload);
    out
}

#[derive(Default)]
struct Sample {
    hex: u64,
    json: u64,
    write: u64,
    total: u64,
}

fn stats(mut v: Vec<u64>) -> Value {
    v.sort_unstable();

    let at = |num: usize, den: usize| -> u64 {
        v[((v.len() - 1) * num) / den]
    };

    let sum: u128 = v.iter().map(|x| *x as u128).sum();

    json!({
        "p50_ns": at(50,100),
        "p95_ns": at(95,100),
        "p99_ns": at(99,100),
        "max_ns": *v.last().unwrap(),
        "mean_ns": (sum / v.len() as u128) as u64,
        "p50_us": at(50,100) as f64 / 1000.0,
        "p95_us": at(95,100) as f64 / 1000.0,
        "p99_us": at(99,100) as f64 / 1000.0,
        "max_us": *v.last().unwrap() as f64 / 1000.0
    })
}

fn once(writer: &mut UnixStream, raw: &[u8]) -> Sample {
    let t0 = Instant::now();

    let raw_hex = hex::encode(raw);

    let t1 = Instant::now();

    let mut frame = String::with_capacity(raw_hex.len() + 96);
    frame.push_str(
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":["0x"#,
    );
    frame.push_str(&raw_hex);
    frame.push_str("\"]}\n");

    let t2 = Instant::now();

    writer.write_all(frame.as_bytes()).unwrap();

    let t3 = Instant::now();

    black_box(frame.len());

    Sample {
        hex: (t1 - t0).as_nanos() as u64,
        json: (t2 - t1).as_nanos() as u64,
        write: (t3 - t2).as_nanos() as u64,
        total: (t3 - t0).as_nanos() as u64,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    assert!(
        args.len() == 2,
        "g05_ipc <g03_rust_output.json>"
    );

    let d: Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();

    let calldata =
        hex::decode(d["calldata"].as_str().unwrap().trim_start_matches("0x"))
            .unwrap();

    let portal = Address::from_str(
        "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0",
    )
    .unwrap();

    let r = hex::decode(
        "66e5778143debc8df45dd1f47d2fb5cea41d4a8c49abd312e0847a30ef4bcda0",
    )
    .unwrap();

    let s = hex::decode(
        "6a90536f139884940cacd213f0aebf1873569889f800cbfc802056329de86771",
    )
    .unwrap();

    // Exact historical legacy Direct-BUY transaction envelope.
    let signed = rlp_list(vec![
        rlp_hex("032e"),
        rlp_hex("127a3980"),
        rlp_hex("16e360"),
        rlp_bytes(portal.as_slice()),
        rlp_hex("0214e8348c4f0000"),
        rlp_bytes(&calldata),
        rlp_hex("93"),
        rlp_bytes(&r),
        rlp_bytes(&s),
    ]);

    let (mut writer, mut reader) = UnixStream::pair().unwrap();

    let receiver = thread::spawn(move || {
        let mut buf = [0u8; 65536];
        let mut total = 0u64;

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => total += n as u64,
                Err(_) => break,
            }
        }

        total
    });

    const WARMUP: usize = 10_000;
    const SAMPLES: usize = 100_000;

    for _ in 0..WARMUP {
        black_box(once(&mut writer, &signed));
    }

    let mut hex_v = Vec::with_capacity(SAMPLES);
    let mut json_v = Vec::with_capacity(SAMPLES);
    let mut write_v = Vec::with_capacity(SAMPLES);
    let mut total_v = Vec::with_capacity(SAMPLES);

    for _ in 0..SAMPLES {
        let s = once(&mut writer, &signed);

        hex_v.push(s.hex);
        json_v.push(s.json);
        write_v.push(s.write);
        total_v.push(s.total);
    }

    writer.shutdown(Shutdown::Write).unwrap();
    let received_bytes = receiver.join().unwrap();

    let raw_hex_len = signed.len() * 2;

    let frame_len =
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_sendRawTransaction","params":["0x"#.len()
        + raw_hex_len
        + "\"]}\n".len();

    let result = json!({
        "status": "G0.5_LOCAL_IPC_PREQUALIFICATION",
        "scope": "Exact historical G0.3 signed Direct-BUY envelope; local preconnected UnixStream only",
        "architecture": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "warmup_samples": WARMUP,
        "measured_samples": SAMPLES,
        "raw_tx_bytes": signed.len(),
        "json_rpc_frame_bytes": frame_len,
        "receiver_total_bytes": received_bytes,
        "stages": {
            "raw_tx_hex_encoding": stats(hex_v),
            "json_rpc_framing": stats(json_v),
            "unix_stream_write_all": stats(write_v),
            "signed_raw_tx_to_local_ipc_handoff": stats(total_v)
        },
        "non_claims": [
            "No real geth process is connected",
            "UnixStream write-return is not bsc-geth P2P propagation",
            "Not final T_send",
            "Not T_buy",
            "No value-bearing transaction was broadcast"
        ]
    });

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
