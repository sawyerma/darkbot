#![cfg(unix)]
// TEST_ONLY E2E: real reporter child process, no Project0 crate linked by gateway.
use std::{path::PathBuf,process::{Child,Command,Stdio},sync::Arc,time::{Duration,SystemTime,UNIX_EPOCH}};
use darkbot_gateway::{config::GatewayConfig,engine::{spawn_supervisor,SourceRegistry,SourceState},feeds::UnixSocketFeed,hub::StateHub,proto::{project0::v1::StatusProjection,ProjectId,StateQuality,TopicId}};
use prost::Message as _;
struct Fixture(Child);
impl Fixture{
 fn spawn(sock:&PathBuf,gen:&PathBuf)->Self{
  let bin=std::env::var_os("DARKBOT_P0_REPORTER_FIXTURE").map(PathBuf::from).unwrap_or_else(||PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../project_0/native/target/release/p0-reporter-fixture"));
  assert!(bin.is_file(),"fixture missing: {}. Build via cargo build --release -p darkbot-p0-reporter --features test-fixture --bin p0-reporter-fixture in project_0/native",bin.display());
  Self(Command::new(bin).env("P0_FIXTURE_SOCKET",sock).env("P0_FIXTURE_GENERATION_FILE",gen).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap())
 }
 fn stop(mut self){drop(self.0.stdin.take());assert!(self.0.wait().unwrap().success());}
}
impl Drop for Fixture{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
async fn wait(mut ok:impl FnMut()->bool,what:&str){
 let deadline=tokio::time::Instant::now()+Duration::from_secs(10);
 while !ok(){assert!(tokio::time::Instant::now()<deadline,"timeout: {what}");tokio::time::sleep(Duration::from_millis(25)).await;}
}
#[tokio::test]
#[ignore="requires external p0-reporter-fixture binary"]
async fn ipc_state_detach_and_reconnect(){
 let nonce=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
 let id=format!("p0e2e-{}-{:x}",std::process::id(),nonce as u64);
 let sock=PathBuf::from(format!("/tmp/{id}.sock"));
 let dir=std::env::temp_dir().join(id);std::fs::create_dir(&dir).unwrap();
 let gen=dir.join("generation");
 let mut cfg=GatewayConfig::from_env().unwrap();cfg.source_reattach_backoff_ms=75;
 let hub=Arc::new(StateHub::new(262144));let registry=SourceRegistry::new();
 let supervisor=spawn_supervisor(UnixSocketFeed::new(ProjectId::Project0,"Project 0",sock.clone(),262144),hub.clone(),registry.clone(),&cfg);
 let one=Fixture::spawn(&sock,&gen);
 wait(||registry.status(ProjectId::Project0).is_some_and(|s|s.state==SourceState::Connected),"connected").await;
 let state=hub.current(TopicId::Project0Status).unwrap();
 assert_eq!(state.generation,1);assert_eq!(state.quality,StateQuality::Ok);
 let payload=StatusProjection::decode(state.payload.as_ref()).unwrap();
 assert_eq!(payload.run_id,"run-TEST_ONLY");assert_eq!(payload.market_verdict,"MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED");
 assert!(payload.queue_bytes.is_some()&&payload.last_observed_slot.is_some()&&payload.last_frame_age_ns.is_some());
 let seq=state.seq;
 wait(||hub.current(TopicId::Project0Status).is_some_and(|s|s.seq>seq),"sequence").await;
 one.stop();
 wait(||registry.status(ProjectId::Project0).is_some_and(|s|s.state==SourceState::Detached),"detached").await;
 assert_eq!(hub.current(TopicId::Project0Status).unwrap().quality,StateQuality::SourceDetached);
 let two=Fixture::spawn(&sock,&gen);
 wait(||hub.current(TopicId::Project0Status).is_some_and(|s|s.generation==2&&s.quality==StateQuality::Ok),"reattached").await;
 two.stop();supervisor.abort();std::fs::remove_dir_all(dir).unwrap();
}
