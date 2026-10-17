use std::{collections::HashMap,time::{Duration,Instant}};
use axum::http::{HeaderMap,HeaderValue,header::{COOKIE,HOST,ORIGIN}};
use base64::Engine as _;
use parking_lot::RwLock;
use rand::RngCore;
use crate::config::GatewayConfig;

#[derive(Clone)]
struct Session { created:Instant, seen:Instant }
pub struct SessionStore { inner:RwLock<HashMap<String,Session>>, cap:usize, ttl:Duration }
impl SessionStore {
 pub fn new(cap:usize,ttl:Duration)->Self { Self{inner:RwLock::new(HashMap::new()),cap,ttl} }
 pub fn issue(&self)->String {
  let token=random_token(32); let now=Instant::now(); let mut g=self.inner.write();
  g.retain(|_,s|now.duration_since(s.seen)<self.ttl);
  if g.len()>=self.cap { if let Some(k)=g.iter().min_by_key(|(_,s)|s.created).map(|(k,_)|k.clone()){g.remove(&k);} }
  g.insert(token.clone(),Session{created:now,seen:now});token
 }
 pub fn validate(&self,token:&str)->bool {
  let now=Instant::now();let mut g=self.inner.write();
  match g.get_mut(token) {Some(s) if now.duration_since(s.seen)<self.ttl=>{s.seen=now;true},Some(_)=>{g.remove(token);false},None=>false}
 }
 pub fn len(&self)->usize {self.inner.read().len()}
 pub fn is_empty(&self)->bool {self.len()==0}
}
pub fn random_token(bytes:usize)->String {let mut b=vec![0;bytes];rand::thread_rng().fill_bytes(&mut b);base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)}
pub fn cookie_value(headers:&HeaderMap,name:&str)->Option<String> {
 headers.get_all(COOKIE).iter().filter_map(|v|v.to_str().ok()).flat_map(|s|s.split(';')).map(str::trim).find_map(|part|{let(k,v)=part.split_once('=')?;(k==name).then(||v.to_owned())})
}
pub fn set_cookie(name:&str,token:&str)->HeaderValue {HeaderValue::from_str(&format!("{name}={token}; Path=/; HttpOnly; SameSite=Strict")).expect("ascii token")}
pub fn has_valid_session(headers:&HeaderMap,cfg:&GatewayConfig,store:&SessionStore)->bool {cookie_value(headers,&cfg.session_cookie).map(|s|store.validate(&s)).unwrap_or(false)}
pub fn check_host(h:&HeaderMap,cfg:&GatewayConfig)->Result<(),&'static str> {match h.get(HOST).and_then(|x|x.to_str().ok()){Some(v) if cfg.host_allowed(v)=>Ok(()),Some(_)=>Err("host not allowed"),None=>Err("host missing")}}
pub fn check_origin(h:&HeaderMap,cfg:&GatewayConfig)->Result<(),&'static str> {match h.get(ORIGIN).and_then(|x|x.to_str().ok()){Some(v) if cfg.origin_allowed(v)=>Ok(()),Some(_)=>Err("origin not allowed"),None=>Err("origin missing")}}
pub fn check_fetch_site(h:&HeaderMap)->Result<(),&'static str> {match h.get("sec-fetch-site").and_then(|x|x.to_str().ok()){None|Some("same-origin")|Some("none")=>Ok(()),_=>Err("cross-site fetch rejected")}}
#[cfg(test)] mod tests {
 use super::*;#[test]fn bounded_session(){let s=SessionStore::new(2,Duration::from_secs(60));let a=s.issue();assert!(s.validate(&a));s.issue();s.issue();assert_eq!(s.len(),2);}
}
