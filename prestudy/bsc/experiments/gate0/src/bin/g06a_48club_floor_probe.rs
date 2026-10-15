use alloy_primitives::{keccak256, Address, U256};
use k256::ecdsa::SigningKey;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

const HOST: &str = "puissant-builder.48.club";
const CHAIN_ID: u64 = 56;
const PORTAL: &str = "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0";

fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 { return b.to_vec(); }
    if b.len() <= 55 {
        let mut o=vec![0x80+b.len() as u8]; o.extend_from_slice(b); return o;
    }
    let lb=(b.len() as u64).to_be_bytes(); let i=lb.iter().position(|x|*x!=0).unwrap();
    let mut o=vec![0xb7+(8-i) as u8]; o.extend_from_slice(&lb[i..]); o.extend_from_slice(b); o
}
fn rlp_u64(v:u64)->Vec<u8>{ if v==0{return rlp_bytes(&[])} let b=v.to_be_bytes(); let i=b.iter().position(|x|*x!=0).unwrap(); rlp_bytes(&b[i..]) }
fn rlp_u256(v:U256)->Vec<u8>{ if v==U256::ZERO{return rlp_bytes(&[])} let b=v.to_be_bytes::<32>(); let i=b.iter().position(|x|*x!=0).unwrap(); rlp_bytes(&b[i..]) }
fn rlp_list(items:&[Vec<u8>])->Vec<u8>{
    let mut p=Vec::new(); for x in items {p.extend_from_slice(x);}
    if p.len()<=55 {let mut o=vec![0xc0+p.len() as u8];o.extend_from_slice(&p);return o;}
    let lb=(p.len() as u64).to_be_bytes();let i=lb.iter().position(|x|*x!=0).unwrap();
    let mut o=vec![0xf7+(8-i) as u8];o.extend_from_slice(&lb[i..]);o.extend_from_slice(&p);o
}
fn sign_dummy(key:&SigningKey,gas_price:u64)->Vec<u8>{
    let to=Address::from_str(PORTAL).unwrap();
    let data=hex::decode("deadbeef").unwrap();
    let unsigned=rlp_list(&[
        rlp_u64(0),rlp_u64(gas_price),rlp_u64(21000),rlp_bytes(to.as_slice()),
        rlp_u256(U256::ZERO),rlp_bytes(&data),rlp_u64(CHAIN_ID),rlp_bytes(&[]),rlp_bytes(&[])
    ]);
    let h=keccak256(unsigned);
    let (sig,rid)=key.sign_prehash_recoverable(h.as_slice()).unwrap();
    let sb=sig.to_bytes(); let v=35+2*CHAIN_ID+rid.to_byte() as u64;
    rlp_list(&[
        rlp_u64(0),rlp_u64(gas_price),rlp_u64(21000),rlp_bytes(to.as_slice()),
        rlp_u256(U256::ZERO),rlp_bytes(&data),rlp_u64(v),
        rlp_u256(U256::from_be_slice(&sb[..32])),rlp_u256(U256::from_be_slice(&sb[32..]))
    ])
}
fn connect()->StreamOwned<ClientConnection,TcpStream>{
    let addr=(HOST,443).to_socket_addrs().unwrap().next().unwrap();
    let mut tcp=TcpStream::connect_timeout(&addr,Duration::from_secs(5)).unwrap();
    tcp.set_nodelay(true).unwrap(); tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let roots=RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg=ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let name=ServerName::try_from(HOST.to_string()).unwrap();
    let mut conn=ClientConnection::new(Arc::new(cfg),name).unwrap(); conn.complete_io(&mut tcp).unwrap();
    StreamOwned::new(conn,tcp)
}
fn http(body:&str)->Vec<u8>{
    format!("POST / HTTP/1.1\r\nHost: {HOST}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",body.len(),body).into_bytes()
}
fn run(gas:u64)->Value{
    let key=SigningKey::from_bytes((&[0x33u8;32]).into()).unwrap();
    let raw=sign_dummy(&key,gas);
    let body=json!({"jsonrpc":"2.0","id":1,"method":"eth_sendBundle","params":[{"txs":[format!("0x{}",hex::encode(raw))],"noMerge":true,"positionFirst":true}]}).to_string();
    let mut s=connect(); s.write_all(&http(&body)).unwrap(); s.flush().unwrap();
    let mut out=Vec::new(); s.read_to_end(&mut out).ok();
    let text=String::from_utf8_lossy(&out).to_string();
    json!({"gas_price_wei":gas.to_string(),"response_tail":text.chars().rev().take(1200).collect::<String>().chars().rev().collect::<String>()})
}
fn main(){
    let vals=[50_000_000u64,100_000_000,500_000_000,1_000_000_000];
    let results:Vec<Value>=vals.into_iter().map(run).collect();
    println!("{}",serde_json::to_string_pretty(&json!({
        "status":"G0.6A_48CLUB_NONEXECUTABLE_FLOOR_PROBE",
        "host":HOST,
        "fixture":"validly signed but fixed unfunded dummy sender; zero value; deliberately non-executable",
        "results":results
    })).unwrap());
}
