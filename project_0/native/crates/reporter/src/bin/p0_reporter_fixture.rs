//! TEST_ONLY reporter fixture. No network or wallet.
use darkbot_p0_reporter::{start,ReporterConfig};
use darkbot_runner::status_tap::{MarketSnapshot,StatusTap,StorageSnapshot,TransportSnapshot};
use std::{io::Read,sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
fn main(){
 let socket=std::env::var("P0_FIXTURE_SOCKET").expect("fixture socket");
 let gen=std::env::var("P0_FIXTURE_GENERATION_FILE").expect("fixture generation");
 let tap=StatusTap::new();
 let mut cfg=ReporterConfig::new(socket.into(),gen.into(),"run-TEST_ONLY".into());
 cfg.heartbeat=Duration::from_millis(150);
 let handle=start(cfg,tap.clone()).expect("fixture reporter start");
 let stop=Arc::new(AtomicBool::new(false));let done=stop.clone();
 std::thread::spawn(move||{let mut buf=Vec::new();let _=std::io::stdin().read_to_end(&mut buf);done.store(true,Ordering::Relaxed);});
 let mut i=0u64;
 while !stop.load(Ordering::Relaxed){
  i+=1;
  tap.publish_transport(TransportSnapshot{connected:true,last_frame_age_ns:Some(1000000*i),last_slot:Some(300000000+i)});
  tap.publish_market(MarketSnapshot{observing:true,stream_ready:true,gap_count:i/50,reconnect_count:0,liquidation_events:i/10,last_observed_slot:Some(300000000+i),live_verdict:"MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"});
  tap.publish_storage(StorageSnapshot{queue_bytes:4096*(i%7),lag_frames:i%3});
  std::thread::sleep(Duration::from_millis(50));
 }
 handle.stop();
}
