//! Post-capture strict verification. Reuses the canonical slot/parent/skip engine.
//! Raw capture files are never opened for writing; only this specification's directory.
use super::reconcile::RequestPacer;
use super::*;
use serde::Deserialize;
use solana_sdk::hash::{hash, Hasher};
use std::io::{BufRead, Read};

#[derive(Deserialize)]
pub struct Job {
    pub verification_spec_id: String,
    pub output: PathBuf,
    pub start_ns: u64,
    pub end_ns: u64,
    pub program: String,
    pub max_calls: u64,
    pub max_seconds: u64,
    pub max_bytes: u64,
    pub max_disk_bytes: u64,
    pub max_ram_bytes: u64,
    pub min_free_bytes: u64,
    pub rps: u64,
    #[serde(default)]
    pub offline: bool,
}
fn digest(v: &Value) -> String {
    hash(&serde_json::to_vec(v).unwrap()).to_string()
}
fn read(path: &Path) -> Result<Value, &'static str> {
    if fs::metadata(path).map_err(|_| "CHECKPOINT_IO")?.len() > 128 * 1024 * 1024 {
        return Err("CHECKPOINT_SIZE");
    }
    serde_json::from_reader(File::open(path).map_err(|_| "CHECKPOINT_IO")?)
        .map_err(|_| "CHECKPOINT_CORRUPT")
}
fn immutable(path: &Path, value: &Value) -> Result<(), &'static str> {
    if path.exists() {
        return if read(path)? == *value {
            Ok(())
        } else {
            Err("IMMUTABLE_CONFLICT")
        };
    }
    let temp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut f = File::create(&temp).map_err(|_| "EVIDENCE_IO")?;
    serde_json::to_writer(&mut f, value).map_err(|_| "EVIDENCE_IO")?;
    f.sync_all().map_err(|_| "EVIDENCE_IO")?;
    fs::hard_link(&temp, path).map_err(|_| "EVIDENCE_IO")?;
    fs::remove_file(temp).map_err(|_| "EVIDENCE_IO")?;
    File::open(path.parent().unwrap())
        .and_then(|d| d.sync_all())
        .map_err(|_| "EVIDENCE_IO")
}
fn file_hash(path: &Path) -> Result<String, &'static str> {
    let mut f = File::open(path).map_err(|_| "EVIDENCE_IO")?;
    let mut h = Hasher::default();
    let mut b = [0; 65536];
    loop {
        let n = f.read(&mut b).map_err(|_| "EVIDENCE_IO")?;
        if n == 0 {
            break;
        }
        h.hash(&b[..n]);
    }
    Ok(h.result().to_string())
}
fn disk_bytes(dir: &Path) -> Result<u64, &'static str> {
    let mut total = 0;
    for e in fs::read_dir(dir).map_err(|_| "RESOURCE_IO")? {
        let e = e.map_err(|_| "RESOURCE_IO")?;
        let m = e.metadata().map_err(|_| "RESOURCE_IO")?;
        total += if m.is_dir() {
            disk_bytes(&e.path())?
        } else {
            m.len()
        };
    }
    Ok(total)
}
struct Client<'a> {
    job: &'a Job,
    rpc: Option<RpcFetcher>,
    deadline: Instant,
    pacer: RequestPacer,
    calls: u64,
    bytes: u64,
}
impl Client<'_> {
    fn guard(&self) -> Result<(), &'static str> {
        if Instant::now() >= self.deadline {
            return Err("VERIFICATION_TIME_BUDGET");
        }
        if disk_bytes(&self.job.output)? >= self.job.max_disk_bytes {
            return Err("VERIFICATION_DISK_BUDGET");
        }
        let rss = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .map_err(|_| "RAM_PREFLIGHT_UNAVAILABLE")?;
        let rss: u64 = String::from_utf8_lossy(&rss.stdout)
            .trim()
            .parse()
            .map_err(|_| "RAM_PREFLIGHT_UNAVAILABLE")?;
        // Reserve headroom for the capped response + serde expansion and decoder clones.
        if rss.saturating_mul(1024).saturating_add(512 * 1024 * 1024) > self.job.max_ram_bytes {
            return Err("VERIFICATION_RAM_BUDGET");
        }
        let df = std::process::Command::new("df")
            .args(["-Pk"])
            .arg(&self.job.output)
            .output()
            .map_err(|_| "DISK_PREFLIGHT_UNAVAILABLE")?;
        let free: u64 = String::from_utf8_lossy(&df.stdout)
            .lines()
            .last()
            .and_then(|s| s.split_whitespace().nth(3))
            .and_then(|s| s.parse().ok())
            .ok_or("DISK_PREFLIGHT_UNAVAILABLE")?;
        if free.saturating_mul(1024) < self.job.min_free_bytes.saturating_add(64 * 1024 * 1024) {
            return Err("VERIFICATION_DISK_FREE");
        }
        Ok(())
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value, &'static str> {
        self.guard()?;
        if !matches!(
            method,
            "getFirstAvailableBlock" | "getSlot" | "getBlocks" | "getBlock"
        ) {
            return Err("VERIFICATION_METHOD_FORBIDDEN");
        }
        let key = json!({"method":method,"params":params});
        let path = self
            .job
            .output
            .join("cache")
            .join(format!("{}.json", digest(&key)));
        if path.exists() {
            let v = read(&path)?;
            if v["request"] != key || v["digest"] != digest(&v["result"]) {
                return Err("CACHE_CORRUPT");
            }
            return Ok(v["result"].clone());
        }
        if self.calls >= self.job.max_calls {
            return Err("VERIFICATION_CALL_BUDGET");
        }
        let remaining_bytes = self.job.max_bytes.saturating_sub(self.bytes);
        if remaining_bytes == 0 {
            return Err("VERIFICATION_NETWORK_BUDGET");
        }
        let rpc = self.rpc.as_ref().ok_or("OFFLINE_CACHE_MISS")?;
        self.pacer.wait(self.deadline)?;
        self.calls += 1;
        // P2 admission, Retry-After/circuit and bounded body reader remain authoritative.
        let (v, bytes) =
            rpc.gap_request_metered(method, params, self.deadline, 10_000, remaining_bytes)?;
        self.bytes += bytes;
        immutable(
            &path,
            &json!({"request":key,"digest":digest(&v),"result":v}),
        )?;
        self.guard()?;
        Ok(v)
    }
    fn header_at_or_after(&mut self, slot: u64, tip: u64) -> Result<(u64, i64), &'static str> {
        let slots = self.call(
            "getBlocks",
            json!([slot,slot.saturating_add(512).min(tip),{"commitment":"finalized"}]),
        )?;
        let s = slots
            .as_array()
            .and_then(|v| v.first())
            .and_then(Value::as_u64)
            .filter(|s| *s >= slot && *s <= tip)
            .ok_or("UTC_BOUNDARY_SLOT_UNAVAILABLE")?;
        let h = self.call("getBlock", json!([s,{"transactionDetails":"none","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":1}]))?;
        Ok((s, h["blockTime"].as_i64().ok_or("UTC_BLOCK_TIME_UNKNOWN")?))
    }
    fn resolve_window(&mut self) -> Result<(u64, u64), &'static str> {
        let first = self
            .call("getFirstAvailableBlock", json!([]))?
            .as_u64()
            .ok_or("ARCHIVE_FIRST_SLOT_UNKNOWN")?;
        let tip = self
            .call("getSlot", json!([{"commitment":"finalized"}]))?
            .as_u64()
            .ok_or("FINALIZED_TIP_INVALID")?;
        let mut lower = self.header_at_or_after(first, tip)?;
        let upper = self.header_at_or_after(tip, tip)?;
        let start = self.job.start_ns as i128;
        let end = self.job.end_ns as i128;
        let ns = |t: i64| t as i128 * 1_000_000_000;
        if ns(lower.1) >= start || ns(upper.1) < end {
            return Err("UTC_WINDOW_NOT_BRACKETED");
        }
        // Lower bound strictly before start. Missing/pruned probe fails closed.
        let mut hi = upper;
        while hi.0 > lower.0 + 1 {
            let mid = lower.0 + (hi.0 - lower.0) / 2;
            let probe = self.header_at_or_after(mid, hi.0)?;
            if probe.0 == hi.0 {
                break;
            }
            if ns(probe.1) < start {
                lower = probe;
            } else {
                hi = probe;
            }
        }
        let lo = lower.0;
        hi = upper;
        while hi.0 > lower.0 + 1 {
            let probe = self.header_at_or_after(lower.0 + (hi.0 - lower.0) / 2, hi.0)?;
            if probe.0 == hi.0 {
                break;
            }
            if ns(probe.1) < end {
                lower = probe;
            } else {
                hi = probe;
            }
        }
        Ok((lo, hi.0))
    }
}

/// Single job, no wallet or send methods. Each committed batch is immutable.
/// A killed batch is replayed from verified cached RPC responses on resume.
pub fn run(job: Job) -> Result<Value, String> {
    if job.start_ns >= job.end_ns
        || job.max_calls == 0
        || job.max_seconds == 0
        || job.max_seconds > 14_400
        || job.max_calls > 100_000
        || !(1..=5).contains(&job.rps)
        || job.max_ram_bytes < 1024 * 1024 * 1024
        || job.max_bytes == 0
    {
        return Err("VERIFICATION_CONFIG_INVALID".into());
    }
    fs::create_dir_all(job.output.join("cache")).map_err(|_| "VERIFICATION_IO")?;
    let mut c = Client {
        rpc: if job.offline {
            None
        } else {
            let url = std::env::var("P0_AUDIT_RPC_URL")
                .or_else(|_| std::env::var("P0_RPC_URL"))
                .map_err(|_| "RPC_URL_MISSING")?;
            Some(RpcFetcher::new(url, 10_000, "finalized").map_err(|_| "RPC_INIT")?)
        },
        deadline: Instant::now() + Duration::from_secs(job.max_seconds),
        pacer: RequestPacer::new(job.rps),
        calls: 0,
        bytes: 0,
        job: &job,
    };
    let work = (|| -> Result<Value, &'static str> {
        c.guard()?;
        let scope_path = job.output.join("scope.json");
        let scope = if scope_path.exists() {
            read(&scope_path)?
        } else {
            let (lo, hi) = c.resolve_window()?;
            let v = json!({"verification_spec_id":job.verification_spec_id,"start_ns":job.start_ns,"end_ns":job.end_ns,"lo":lo,"hi":hi});
            immutable(&scope_path, &v)?;
            v
        };
        if scope["verification_spec_id"] != job.verification_spec_id
            || scope["start_ns"] != job.start_ns
            || scope["end_ns"] != job.end_ns
        {
            return Err("RESUME_SCOPE_MISMATCH");
        }
        let mut next = scope["lo"].as_u64().ok_or("CHECKPOINT_CORRUPT")?;
        let hi = scope["hi"]
            .as_u64()
            .filter(|v| *v >= next)
            .ok_or("CHECKPOINT_CORRUPT")?;
        let mut anchor: Option<(u64, String)> = None;
        let mut batches = Vec::new();
        let mut first_time = None;
        let mut last_time = None;
        while next <= hi {
            c.guard()?;
            let end = next.saturating_add(31).min(hi);
            let checkpoint = job.output.join(format!("batch_{next}_{end}.json"));
            let saved = if checkpoint.exists() {
                read(&checkpoint)?
            } else {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "CLOCK")?
                    .as_nanos();
                let dirname = format!("batch_{next}_{end}_{stamp}");
                let dir = job.output.join(&dirname);
                fs::create_dir(&dir).map_err(|_| "EVIDENCE_IO")?;
                let mut sink = JsonlSink::create(&dir).map_err(|_| "EVIDENCE_IO")?;
                let mut core = ObserverCore::new(
                    Pubkey::from_str(&job.program).map_err(|_| "PROGRAM_INVALID")?,
                    MarketSource::Transactions,
                    EnrichConfig::DEFAULT,
                );
                core.reconcile_cfg.max_attempts = 1;
                core.reconcile_cfg.phase_max_secs = c
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs()
                    .max(1);
                let start = anchor.as_ref().map(|a| a.0).unwrap_or(next);
                core.reconciliation.market_slot_start = Some(reconcile::SlotBoundary {
                    slot: start,
                    t_rx_ns: 0,
                    observed_unix_s: 0,
                    provenance: "UTC_BRACKET_RESUMABLE_STRICT",
                });
                core.reconciliation.market_slot_end = Some(reconcile::SlotBoundary {
                    slot: end,
                    t_rx_ns: 0,
                    observed_unix_s: 0,
                    provenance: "UTC_BRACKET_RESUMABLE_STRICT",
                });
                core.reconcile_slots(
                    &mut |m, p, _| {
                        let value = c.call(m, p.clone())?;
                        if m == "getBlock" {
                            if let Some((s, h)) = &anchor {
                                if p[0] == *s && value["blockhash"] != *h {
                                    return Err("BATCH_ANCHOR_MISMATCH");
                                }
                            }
                        }
                        Ok(value)
                    },
                    &mut sink,
                )
                .map_err(|_| "EVIDENCE_IO")?;
                sink.flush().map_err(|_| "EVIDENCE_IO")?;
                if !core.reconciliation.complete {
                    return Err(core.reconciliation.error.unwrap_or("STRICT_INCOMPLETE"));
                }
                let mut last = anchor.clone();
                for line in std::io::BufReader::new(
                    File::open(dir.join("coverage.jsonl")).map_err(|_| "EVIDENCE_IO")?,
                )
                .lines()
                {
                    let row: Value = serde_json::from_str(&line.map_err(|_| "EVIDENCE_IO")?)
                        .map_err(|_| "EVIDENCE_IO")?;
                    if row["status"] == "PRODUCED_FETCHED" {
                        last = Some((
                            row["slot"].as_u64().ok_or("PROOF_SLOT")?,
                            row["blockhash"].as_str().ok_or("PROOF_HASH")?.into(),
                        ));
                    }
                }
                let mut files = serde_json::Map::new();
                for name in ["transactions.jsonl", "liquidations.jsonl", "coverage.jsonl"] {
                    File::open(dir.join(name))
                        .and_then(|f| f.sync_all())
                        .map_err(|_| "EVIDENCE_IO")?;
                    files.insert(name.into(), json!(file_hash(&dir.join(name))?));
                }
                let v = json!({"spec":job.verification_spec_id,"start":next,"end":end,"previous_anchor":anchor,"anchor":last,
                    "directory":dirname,"files":files,"proof":core.reconciliation});
                immutable(&checkpoint, &v)?;
                v
            };
            if saved["spec"] != job.verification_spec_id
                || saved["start"] != next
                || saved["end"] != end
                || saved["previous_anchor"] != json!(anchor)
                || saved["proof"]["complete"] != true
            {
                return Err("CHECKPOINT_CORRUPT");
            }
            let dirname = saved["directory"]
                .as_str()
                .filter(|s| !s.contains('/') && !s.contains('\\') && s.starts_with("batch_"))
                .ok_or("CHECKPOINT_PATH")?;
            for name in ["transactions.jsonl", "liquidations.jsonl", "coverage.jsonl"] {
                if saved["files"][name] != file_hash(&job.output.join(dirname).join(name))? {
                    return Err("CHECKPOINT_EVIDENCE_CHANGED");
                }
            }
            anchor = serde_json::from_value(saved["anchor"].clone())
                .map_err(|_| "CHECKPOINT_CORRUPT")?;
            if first_time.is_none() {
                first_time = saved["proof"]["first_block_time"].as_i64();
            }
            last_time = saved["proof"]["last_block_time"].as_i64().or(last_time);
            batches.push(json!({"checkpoint":checkpoint.file_name().unwrap().to_string_lossy(),"digest":digest(&saved)}));
            next = end.checked_add(1).ok_or("SLOT_OVERFLOW")?;
        }
        if !first_time.is_some_and(|v| (v as i128) * 1_000_000_000 < job.start_ns as i128)
            || !last_time.is_some_and(|v| (v as i128) * 1_000_000_000 >= job.end_ns as i128)
        {
            return Err("UTC_WINDOW_NOT_BRACKETED");
        }
        Ok(
            json!({"status":"CANONICAL_VERIFIED","scope":scope,"batches":batches,"canonical_verified":true}),
        )
    })();
    let mut result = work.unwrap_or_else(
        |error| json!({"status":"HISTORICAL_INCOMPLETE","reason":error,"canonical_verified":false}),
    );
    result["verification_spec_id"] = json!(job.verification_spec_id);
    result["rpc_calls"] = json!(c.calls);
    result["successful_response_bytes"] = json!(c.bytes);
    result["network_bytes_on_failed_request"] = Value::Null;
    result["c0_e1_master_pass"] = json!(false);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job() -> Job {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Job {
            verification_spec_id: "fixture".into(),
            output: std::env::temp_dir().join(format!("p4-{stamp}")),
            start_ns: 11_000_000_000,
            end_ns: 79_000_000_000,
            program: darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            max_calls: 100,
            max_seconds: 60,
            max_bytes: 1024 * 1024,
            max_disk_bytes: 1024 * 1024 * 1024,
            max_ram_bytes: 4 * 1024 * 1024 * 1024,
            min_free_bytes: 1024 * 1024,
            rps: 2,
            offline: true,
        }
    }
    fn cache(j: &Job, method: &str, params: Value, result: Value) {
        fs::create_dir_all(j.output.join("cache")).unwrap();
        let key = json!({"method":method,"params":params});
        immutable(
            &j.output
                .join("cache")
                .join(format!("{}.json", digest(&key))),
            &json!({"request":key,"digest":digest(&result),"result":result}),
        )
        .unwrap();
    }
    fn fixture(j: &Job, stop: Option<u64>) {
        fs::create_dir_all(&j.output).unwrap();
        immutable(&j.output.join("scope.json"),&json!({"verification_spec_id":"fixture","start_ns":j.start_ns,"end_ns":j.end_ns,"lo":10,"hi":80})).unwrap();
        cache(j, "getSlot", json!([{"commitment":"finalized"}]), json!(90));
        for lo in [10, 41, 73] {
            cache(
                j,
                "getBlocks",
                json!([lo,90,{"commitment":"finalized"}]),
                json!((lo..=90)
                    .filter(|s| *s != 15 && *s != 79)
                    .collect::<Vec<_>>()),
            );
        }
        let mut parent = 9;
        for slot in 10..=90 {
            if slot == 15 || slot == 79 {
                continue;
            }
            if stop.is_none_or(|stop| slot <= stop) {
                cache(
                    j,
                    "getBlock",
                    json!([slot,{"encoding":"json","transactionDetails":"full","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":1}]),
                    json!({"parentSlot":parent,"previousBlockhash":format!("h{parent}"),"blockhash":format!("h{slot}"),"blockTime":slot,"transactions":[]}),
                );
            }
            parent = slot;
        }
    }
    fn reload(j: &Job) -> Job {
        Job {
            verification_spec_id: j.verification_spec_id.clone(),
            output: j.output.clone(),
            start_ns: j.start_ns,
            end_ns: j.end_ns,
            program: j.program.clone(),
            max_calls: j.max_calls,
            max_seconds: j.max_seconds,
            max_bytes: j.max_bytes,
            max_disk_bytes: j.max_disk_bytes,
            max_ram_bytes: j.max_ram_bytes,
            min_free_bytes: j.min_free_bytes,
            rps: j.rps,
            offline: true,
        }
    }
    #[test]
    fn interrupted_batches_resume_identically_with_parent_skip_proofs() {
        let j = job();
        fixture(&j, Some(43));
        let first = run(reload(&j)).unwrap();
        assert_eq!(first["status"], "HISTORICAL_INCOMPLETE");
        let checkpoint = j.output.join("batch_10_41.json");
        assert!(checkpoint.exists());
        let before = file_hash(&checkpoint).unwrap();
        fixture(&j, None);
        let resumed = run(reload(&j)).unwrap();
        assert_eq!(resumed["status"], "CANONICAL_VERIFIED", "{resumed}");
        assert_eq!(resumed["rpc_calls"], 0);
        assert_eq!(file_hash(&checkpoint).unwrap(), before);
        let repeated = run(reload(&j)).unwrap();
        assert_eq!(resumed, repeated);
        assert_eq!(resumed["batches"].as_array().unwrap().len(), 3);
        fs::write(checkpoint, b"broken").unwrap();
        let bad = run(reload(&j)).unwrap();
        assert_eq!(bad["reason"], "CHECKPOINT_CORRUPT");
        fs::remove_dir_all(j.output).unwrap();
    }
    #[test]
    fn disk_budget_and_changed_scope_fail_closed() {
        let mut j = job();
        fixture(&j, None);
        j.max_disk_bytes = 1;
        assert_eq!(
            run(reload(&j)).unwrap()["reason"],
            "VERIFICATION_DISK_BUDGET"
        );
        j.max_disk_bytes = 1024 * 1024 * 1024;
        j.start_ns += 1;
        assert_eq!(run(reload(&j)).unwrap()["reason"], "RESUME_SCOPE_MISMATCH");
        fs::remove_dir_all(j.output).unwrap();
    }
    #[test]
    fn wrong_parent_and_corrupt_cached_response_never_pass() {
        let j = job();
        fixture(&j, None);
        let key = json!({"method":"getBlock","params":[42,{"encoding":"json","transactionDetails":"full","rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":1}]});
        let p = j
            .output
            .join("cache")
            .join(format!("{}.json", digest(&key)));
        let mut value = read(&p).unwrap();
        value["result"]["parentSlot"] = json!(40);
        fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(run(reload(&j)).unwrap()["reason"], "CACHE_CORRUPT");
        value["digest"] = json!(digest(&value["result"]));
        fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            run(reload(&j)).unwrap()["reason"],
            "FINALIZED_PARENT_CHAIN_MISMATCH"
        );
        fs::remove_dir_all(j.output).unwrap();
    }
}
