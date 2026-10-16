from pathlib import Path
import re,difflib,subprocess
r=Path('/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot');a=Path(__file__).parent;root='project_0/native/crates/runtime/src/';u={}
u[root+'analytics/journal.rs']=(a/'journal.rs').read_text()
u[root+'analytics/journal_tests.rs']=(a/'journal_tests.rs').read_text()
p=root+'analytics/mod.rs';u[p]=(r/p).read_text()+'\npub mod journal;\n'
p=root+'analytics/evidence.rs';s=(r/p).read_text()
s=s.replace('    fs::{File, OpenOptions},\n    io::Write,\n','')
s=s.replace('use serde_json::{json, Value};','use serde_json::{json, Value};\nuse super::journal::{self,Writer,Provenance};')
start=s.index('impl OwnerEvidence {');end=s.index('/// Static category',start)
s=s[:start]+'''impl OwnerEvidence {
    fn encode(self) -> Result<(u16,Value),serde_json::Error> {
        match self {
            Self::Record(v)=>(Ok((100,v.value()))),
            Self::Opportunity(v)=>Ok((101,serde_json::to_value(v)?)),
            Self::Reconcile(v)=>Ok((102,serde_json::to_value(v)?)),
            Self::Summary(v)=>Ok((103,serde_json::to_value(v)?)),
        }
    }
}
struct Terminal {refused:Option<OwnerEvidence>,summary:Option<DaySummary>,incomplete:bool}

'''+s[end:]
s=s.replace('        records: File,\n        summaries: File,','        path: PathBuf,')
s=s.replace('        let (tx, rx) = mpsc::sync_channel(capacity);','        let journal=Writer::create(&path)?;\n        let (tx, rx) = mpsc::sync_channel(capacity);',1)
s=s.replace('run_writer(records, summaries, recovery, signal, rx, terminal_rx)','run_writer(journal, recovery, signal, rx, terminal_rx)')
s=s.replace('                summary,\n            })','                summary,\n                incomplete:self.admission_error.is_some() || self.failed.load(Ordering::Acquire),\n            })')
start=s.index('struct Sink {')
s=s[:start]+'''struct Sink {
    journal:Writer,recovery:PathBuf,recovery_writer:Option<Writer>,
    failed:Arc<AtomicBool>,io_failed:bool,recovery_failed:bool,
}
impl Sink {
    fn write(&mut self,event:OwnerEvidence) {
        let(kind,value)=match event.encode(){Ok(v)=>v,Err(_)=>{self.failed.store(true,Ordering::Release);self.recovery_failed=true;return;}};
        let offset=self.journal.position();
        let p=Provenance {source:1,source_seq:self.journal.next_seq(),slot:value.get("slot").or_else(||value.get("rx_slot")).and_then(Value::as_u64),rx_ns:value.get("t_rx_ns").or_else(||value.pointer("/timing/t_rx_ns")).and_then(Value::as_u64),..Default::default()};
        if !self.io_failed && self.journal.append(kind,1,p,&value).is_ok(){return;}
        self.io_failed=true;self.failed.store(true,Ordering::Release);
        if self.recovery_writer.is_none()&&!self.recovery_failed {
            match Writer::create(&self.recovery) {Ok(w)=>self.recovery_writer=Some(w),Err(_)=>self.recovery_failed=true}
        }
        if let Some(w)=&mut self.recovery_writer {
            let value=json!({"original_journal_offset":offset,"event_kind":kind,"event":value});
            if w.append_value(1000,&value).is_err(){self.recovery_failed=true;}
        }
    }
    fn finish(mut self,incomplete:bool)->Result<(),&'static str> {
        if let Some(w)=&mut self.recovery_writer {if w.seal(false).is_err(){self.recovery_failed=true;}}
        if self.recovery_failed{return Err("OWNER_EVIDENCE_RECOVERY_FAILED");}
        if self.io_failed{return Err("OWNER_EVIDENCE_WRITE_FAILED_RECOVERY_SAVED");}
        self.journal.seal(!incomplete).map_err(|_|"OWNER_EVIDENCE_SYNC_FAILED")?;Ok(())
    }
}
fn run_writer(journal:Writer,recovery:PathBuf,signal:Arc<AtomicBool>,rx:mpsc::Receiver<OwnerEvidence>,terminal_rx:mpsc::Receiver<Terminal>)->Result<(),&'static str> {
    let mut sink=Sink {journal,recovery,recovery_writer:None,failed:signal,io_failed:false,recovery_failed:false};
    for event in rx {sink.write(event);}
    let mut incomplete=true;
    if let Ok(end)=terminal_rx.recv() {
        incomplete=end.incomplete;
        if let Some(event)=end.refused {sink.write(event);}
        if let Some(day)=end.summary {sink.write(OwnerEvidence::Summary(day));}
    }
    sink.finish(incomplete)
}
#[cfg(test)]
#[path="evidence_tests.rs"]
mod tests;
'''
u[p]=s
p=root+'orchestration/state_owner/setup.rs';s=(r/p).read_text()
start=s.index('    let summary_path = config::parsed_or::<String>(');end=s.index('    if let Some(report)',start)
s=s[:start]+'''    let record_path = config::parsed_or::<String>("P0_RECORD_PATH", "project_0/data/shadow/opportunities.p0j".into())?;
    let journal_path = crate::analytics::journal::binary_path(std::path::Path::new(&record_path));
    std::fs::create_dir_all(journal_path.parent().unwrap_or(std::path::Path::new(".")))?;
'''+s[end:]
start=s.index('    let recovery_path =');end=s.index('    let counts =',start)
s=s[:start]+'''    let recovery_path = journal_path.with_extension(format!("recovery-{}-{}.p0j",std::process::id(),clock.now_ns()));
    let evidence=OwnerEvidenceWriter::start(journal_path,recovery_path,crate::analytics::evidence::OWNER_EVIDENCE_CAPACITY)?;

'''+s[end:]
u[p]=s
# Test failure injection changes the actual OS file handle, not the storage algorithm.
p=root+'analytics/journal.rs';s=u[p];index=s.index('    pub fn next_seq(')
s=s[:index]+'''    #[cfg(test)]
    pub(crate) fn test_read_only(&mut self) {self.file=File::open(&self.path).unwrap();}
'''+s[index:];u[p]=s
# Adapt prior JSONL parity tests to explicit offline rendering of binary records.
p=root+'analytics/evidence_tests.rs';s=(r/p).read_text()
start=s.index('    let records = File::create');end=s.index('    let (tx, rx)',start)
s=s[:start]+'''    let mut journal=Writer::create(&dir.join("events.p0j")).unwrap();
    if read_only {journal.test_read_only();}
    let recovery=dir.join("recovery.p0j");
'''+s[end:]
s=s.replace('run_writer(records, summaries, recovery, signal, rx, terminal_rx)','run_writer(journal, recovery, signal, rx, terminal_rx)')
# Replace IO assertions wholesale in first 4 tests while preserving tested scenarios.
start=s.index('#[test]');end=s.index('fn parity(',start)
s=s[:start]+'''#[test]
fn full_queue_retains_refused_record_drains_fifo_and_fails_closed() {
 let dir=directory();let(mut w,gate)=paused(&dir,false);let first=w.emit(event("first"));let second=w.emit(event("second"));gate.wait();assert!(first.is_ok());assert_eq!(second.unwrap_err().0,"OWNER_EVIDENCE_OVERFLOW");assert_eq!(w.finish(Some(DaySummary::default())).unwrap_err().0,"OWNER_EVIDENCE_OVERFLOW");
 let rows=journal::values(&dir.join("events.p0j")).unwrap();assert_eq!(rows.len(),3);assert_eq!(rows[0]["reason"],"first");assert_eq!(rows[1]["reason"],"second");
 let mut reader=journal::Reader::open(&dir.join("events.p0j")).unwrap();while reader.next_frame().unwrap().is_some(){}assert!(reader.sealed);assert!(!reader.complete);assert!(!dir.join("recovery.p0j").exists());std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn real_write_failure_preserves_queue_refusal_and_final_summary_in_recovery() {
 let dir=directory();let(mut w,gate)=paused(&dir,true);let _=w.emit(event("first"));let _=w.emit(event("second"));gate.wait();assert_eq!(w.finish(Some(DaySummary::default())).unwrap_err().0,"OWNER_EVIDENCE_WRITE_FAILED_RECOVERY_SAVED");
 let rows=journal::values(&dir.join("recovery.p0j")).unwrap();assert_eq!(rows.len(),3);assert_eq!(rows[0]["event"]["reason"],"first");assert_eq!(rows[1]["event"]["reason"],"second");assert_eq!(rows[2]["event_kind"],103);std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn early_error_drop_joins_writer_and_does_not_abandon_accepted_records() {
 let dir=directory();let(mut w,gate)=paused(&dir,false);let sent=w.emit(event("accepted"));gate.wait();assert!(sent.is_ok());drop(w);assert_eq!(journal::values(&dir.join("events.p0j")).unwrap()[0]["reason"],"accepted");std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn recovery_destination_collision_is_not_overwritten_or_reported_as_success() {
 let dir=directory();std::fs::write(dir.join("recovery.p0j"),b"existing").unwrap();let(mut w,gate)=paused(&dir,true);let _=w.emit(event("failed"));gate.wait();assert_eq!(w.finish(None).unwrap_err().0,"OWNER_EVIDENCE_RECOVERY_FAILED");assert_eq!(std::fs::read(dir.join("recovery.p0j")).unwrap(),b"existing");std::fs::remove_dir_all(dir).unwrap();
}
'''+s[end:]
s=s.replace('let (_, bytes) = OwnerEvidence::Record(event).encode().unwrap();','let (_, value) = OwnerEvidence::Record(event).encode().unwrap();\n    let bytes=serde_json::to_vec(&journal::decode(&journal::encode(&value).unwrap()).unwrap()).unwrap();')
s=s.replace('format!("{expected}\\n").as_bytes()','format!("{expected}").as_bytes()')
start=s.index('    let dir = directory();',s.index('fn ordinary_writer'))
s=s[:start]+'''    let dir=directory();let path=dir.join("events.p0j");let mut writer=OwnerEvidenceWriter::start(path.clone(),dir.join("recovery.p0j"),4).unwrap();
    writer.emit(OwnerEvidence::Opportunity(Box::new(record))).unwrap();writer.emit(OwnerEvidence::Reconcile(reconciled)).unwrap();writer.finish(Some(summary)).unwrap();
    let values=journal::values(&path).unwrap();
    // JSON object order is canonical in the binary codec; field values and nulls remain exact.
    let expected:Vec<Value>=expected_records.lines().map(|l|serde_json::from_str(l).unwrap()).collect();assert_eq!(&values[..2],&expected);
    assert_eq!(values[2],serde_json::from_str::<Value>(&expected_summary).unwrap());
    assert!(!dir.join("recovery.p0j").exists());std::fs::remove_dir_all(dir).unwrap();
}
''';u[p]=s
patch=''
for p,s in u.items():
 result=subprocess.run(['rustfmt','--edition','2021','--config','skip_children=true','--emit','stdout'],input=s,text=True,capture_output=True)
 if result.returncode: print(p,result.stderr);raise SystemExit(1)
 s=result.stdout
 old=(r/p).read_text() if (r/p).exists() else ''
 patch+=''.join(difflib.unified_diff(old.splitlines(True),s.splitlines(True),fromfile='a/'+p if old else '/dev/null',tofile='b/'+p))
(a/'owner.patch').write_text(patch)
print(len(u),'files')
