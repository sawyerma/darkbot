//! F3 integration tests: real TCP/WebSocket, no engine data fabrication.
use std::{net::SocketAddr,sync::Arc,time::Duration};
use bytes::Bytes;
use darkbot_gateway::{config::{GatewayConfig,Limits},engine::SourceRegistry,http::{self,AppState},hub::{StateHub,StateUpdate},proto::{client_message,server_message,ClientMessage,ServerMessage,Hello,Subscribe,TopicId,WelcomeStatus,AckStatus,StateQuality},PROTOCOL_MAJOR,PROTOCOL_MINOR,SCHEMA_ID};
use prost::Message as _;
use futures_util::{SinkExt,StreamExt};
use tokio::{net::{TcpListener,TcpStream},io::{AsyncReadExt,AsyncWriteExt}};
use tokio_tungstenite::{MaybeTlsStream,WebSocketStream,tungstenite::{client::IntoClientRequest,Message,Error as WsError}};
type Ws=WebSocketStream<MaybeTlsStream<TcpStream>>;
struct TestServer{addr:SocketAddr,state:Arc<AppState>,hub:Arc<StateHub>}
impl TestServer{
 fn host(&self)->String{format!("127.0.0.1:{}",self.addr.port())}
 fn origin(&self)->String{format!("http://{}",self.host())}
 fn url(&self)->String{format!("ws://{}/ws/v1/gateway",self.host())}
}
async fn spawn(mut limits:Limits)->TestServer{
 limits.handshake_timeout_ms=350;
 let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let addr=listener.local_addr().unwrap();
 let cfg=GatewayConfig{bind:addr,static_dir:None,allowed_origins:vec![format!("http://{addr}")],allowed_hosts:vec![addr.to_string()],session_cookie:"darkbot_session".into(),limits,source_stale_after_ms:1000,source_detach_after_ms:2000,source_reattach_backoff_ms:50,overview_interval_ms:60000};
 let hub=Arc::new(StateHub::new(1024));let registry=SourceRegistry::new();
 let state=AppState::new(cfg,hub.clone(),registry);
 let server_state = Arc::clone(&state);
 tokio::spawn(async move { axum::serve(listener, http::router(server_state)).await.expect("test server"); });
 TestServer{addr,state,hub}
}
async fn try_connect(s:&TestServer,cookie:bool,origin:Option<&str>,host:Option<&str>)->Result<Ws,WsError>{
 let mut req=s.url().into_client_request().unwrap();
 req.headers_mut().insert("origin",origin.unwrap_or(&s.origin()).parse().unwrap());
 if let Some(host)=host{req.headers_mut().insert("host",host.parse().unwrap());}
 if cookie{let token=s.state.sessions.issue();req.headers_mut().insert("cookie",format!("darkbot_session={token}").parse().unwrap());}
 tokio_tungstenite::connect_async(req).await.map(|(ws,_)|ws)
}
async fn connect(s:&TestServer)->Ws{try_connect(s,true,None,None).await.unwrap()}
async fn send(ws:&mut Ws,m:client_message::Msg){ws.send(Message::Binary(ClientMessage{msg:Some(m)}.encode_to_vec())).await.unwrap();}
async fn recv(ws:&mut Ws)->Option<server_message::Msg>{
 loop{let item=tokio::time::timeout(Duration::from_secs(3),ws.next()).await.expect("receive timeout")?;
 match item{Ok(Message::Binary(b))=>return ServerMessage::decode(b.as_slice()).unwrap().msg,Ok(Message::Close(_))|Err(_)=>return None,_=>{}}}
}
async fn handshake(ws:&mut Ws)->darkbot_gateway::proto::Welcome{
 send(ws,client_message::Msg::Hello(Hello{protocol_major:PROTOCOL_MAJOR,protocol_minor:PROTOCOL_MINOR,schema_id:SCHEMA_ID.into(),capabilities:None})).await;
 match recv(ws).await{Some(server_message::Msg::Welcome(w))=>w,other=>panic!("expected Welcome: {other:?}")}
}
async fn subscribe(ws:&mut Ws)->darkbot_gateway::proto::ControlAck{
 send(ws,client_message::Msg::Subscribe(Subscribe{request_id:1,topic:TopicId::OverviewAggregates as i32,cursor:None,start_policy:0})).await;
 match recv(ws).await{Some(server_message::Msg::ControlAck(a))=>a,other=>panic!("expected Ack: {other:?}")}
}
async fn state(ws:&mut Ws)->darkbot_gateway::proto::StateFull{
 loop{match recv(ws).await{Some(server_message::Msg::StateFull(s))=>return s,Some(server_message::Msg::Heartbeat(h))=>send(ws,client_message::Msg::Pong(darkbot_gateway::proto::Pong{heartbeat_id:h.heartbeat_id})).await,other=>panic!("expected state: {other:?}")}}
}
fn update(g:u64,s:u64)->StateUpdate{StateUpdate{topic:TopicId::OverviewAggregates,generation:g,seq:s,source_timestamp_unix_ns:1,source_id:Arc::from("TEST_ONLY"),payload:Bytes::from_static(b"\x08\x01")}}
#[tokio::test]async fn accepted_handshake_and_topics(){
 let srv=spawn(Limits::default()).await;let mut ws=connect(&srv).await;let w=handshake(&mut ws).await;
 assert_eq!(w.status,WelcomeStatus::Accepted as i32);assert!(w.topics.iter().any(|x|x.topic==TopicId::OverviewAggregates as i32&&x.available));
 assert!(w.topics.iter().any(|x|x.topic==TopicId::BscStatus as i32&&!x.available));
}
#[tokio::test]async fn major_and_schema_mismatch_rejected(){
 let s=spawn(Limits::default()).await;
 for (major,schema,expected) in [(PROTOCOL_MAJOR+1,SCHEMA_ID,WelcomeStatus::RejectedMajor),(PROTOCOL_MAJOR,"incorrect-schema",WelcomeStatus::RejectedSchema)]{
 let mut ws=connect(&s).await;send(&mut ws,client_message::Msg::Hello(Hello{protocol_major:major,protocol_minor:PROTOCOL_MINOR,schema_id:schema.into(),capabilities:None})).await;
 match recv(&mut ws).await{Some(server_message::Msg::Welcome(w))=>assert_eq!(w.status,expected as i32),other=>panic!("{other:?}")}
 }
}
#[tokio::test]async fn cookie_origin_host_guards(){
 let s=spawn(Limits::default()).await;
 for (cookie,origin,host,expected) in [(false,None,None,401),(true,Some("http://evil.example"),None,403),(true,None,Some("evil.example"),403)]{
 let err=try_connect(&s,cookie,origin,host).await.err().expect("must reject");
 match err{WsError::Http(res)=>assert_eq!(res.status().as_u16(),expected),other=>panic!("{other:?}")}
 }
}
#[tokio::test]async fn session_bootstrap_and_cross_site_rejection(){
 let s=spawn(Limits::default()).await;
 for (site,status) in [("same-origin",204),("cross-site",403)]{
 let mut conn=TcpStream::connect(s.addr).await.unwrap();
 conn.write_all(format!("GET /api/v1/session HTTP/1.1\r\nHost: {}\r\nSec-Fetch-Site: {site}\r\nConnection: close\r\n\r\n",s.host()).as_bytes()).await.unwrap();
 let mut b=Vec::new();conn.read_to_end(&mut b).await.unwrap();let r=String::from_utf8_lossy(&b);
 assert!(r.starts_with(&format!("HTTP/1.1 {status}")),"{r}");
 if status==204{assert!(r.to_ascii_lowercase().contains("httponly"));assert!(r.contains("SameSite=Strict"));}
 }
}
#[tokio::test]async fn subscription_and_latest_generation(){
 let s=spawn(Limits::default()).await;s.hub.publish(update(1,1)).unwrap();
 let mut ws=connect(&s).await;handshake(&mut ws).await;assert_eq!(subscribe(&mut ws).await.status,AckStatus::Accepted as i32);
 assert_eq!((state(&mut ws).await.generation,state_seq(&s)),(1,1));
 s.hub.publish(update(1,2)).unwrap();assert_eq!(state(&mut ws).await.seq,2);
 s.hub.publish(update(0,99)).unwrap();s.hub.publish(update(2,1)).unwrap();let v=state(&mut ws).await;assert_eq!((v.generation,v.seq),(2,1));
}
fn state_seq(s:&TestServer)->u64{s.hub.current(TopicId::OverviewAggregates).unwrap().seq}
#[tokio::test]async fn unavailable_topic_rejected(){
 let s=spawn(Limits::default()).await;let mut ws=connect(&s).await;handshake(&mut ws).await;
 send(&mut ws,client_message::Msg::Subscribe(Subscribe{request_id:9,topic:TopicId::BscStatus as i32,cursor:None,start_policy:0})).await;
 match recv(&mut ws).await{Some(server_message::Msg::ControlAck(a))=>assert_eq!(a.status,AckStatus::Rejected as i32),other=>panic!("{other:?}")}
}
#[tokio::test]async fn coalescing_at_watch_boundary(){
 let(tx,rx)=tokio::sync::watch::channel(0u64);let mut stream=tokio_stream::wrappers::WatchStream::new(rx);
 assert_eq!(stream.next().await,Some(0));for i in 1..=1000{tx.send(i).unwrap();}
 assert_eq!(stream.next().await,Some(1000));assert!(tokio::time::timeout(Duration::from_millis(30),stream.next()).await.is_err());
}
#[tokio::test]async fn detached_overlay_persists_for_new_subscriber(){
 let s=spawn(Limits::default()).await;s.hub.publish(update(5,7)).unwrap();
 s.hub.mark_detached(darkbot_gateway::proto::ProjectId::Unspecified);
 let mut ws=connect(&s).await;handshake(&mut ws).await;subscribe(&mut ws).await;
 let frame=state(&mut ws).await;assert_eq!(frame.quality,StateQuality::SourceDetached as i32);
}
#[tokio::test]async fn reconnect_gets_latest(){
 let s=spawn(Limits::default()).await;s.hub.publish(update(1,2)).unwrap();
 let mut a=connect(&s).await;handshake(&mut a).await;subscribe(&mut a).await;assert_eq!(state(&mut a).await.seq,2);
 a.close(None).await.unwrap();s.hub.publish(update(1,3)).unwrap();
 let mut b=connect(&s).await;handshake(&mut b).await;subscribe(&mut b).await;assert_eq!(state(&mut b).await.seq,3);
}
