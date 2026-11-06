import sys
p = sys.argv[1]
s = open(p).read()
def rep(old, new, count=1):
    global s
    n = s.count(old)
    assert n == count, (old[:60], n)
    s = s.replace(old, new)
rep("        if Instant::now() >= next_reconcile || stopping {\n            next_reconcile",
    "        let diag_rec = Instant::now();\n        if Instant::now() >= next_reconcile || stopping {\n            next_reconcile")
rep("        // Events, bounded per pass so status and stop stay timely.\n",
    "        diag_t(\"reconcile\", diag_rec);\n        // Events, bounded per pass so status and stop stay timely.\n")
rep("            // The canonical events do not wait for the pass (Phase I gate\n",
    "            let diag_d = Instant::now();\n            // The canonical events do not wait for the pass (Phase I gate\n")
rep("            let wait = deadline\n                .saturating_duration_since(Instant::now())\n                .min(canonical_slice);\n",
    "            diag_t(\"drain_canonical\", diag_d);\n            let wait = deadline\n                .saturating_duration_since(Instant::now())\n                .min(canonical_slice);\n")
rep("            core.queued(source, clock.now_ns().saturating_sub(at));\n",
    "            core.queued(source, clock.now_ns().saturating_sub(at));\n            let diag_ev = Instant::now();\n            let diag_wait = clock.now_ns().saturating_sub(at);\n            if diag_wait >= 1_000_000 {\n                eprintln!(\"DIAG {} queue {} {}us\", diag_ms(), source.name(), diag_wait / 1000);\n            }\n")
rep("\n        }\n        // Canonical events: every queued one, then the joins that are due.\n",
    "\n            diag_t(\"event\", diag_ev);\n        }\n        let diag_c = Instant::now();\n        // Canonical events: every queued one, then the joins that are due.\n")
rep("        if Instant::now() >= next_status || stopping {\n",
    "        diag_t(\"canonical\", diag_c);\n        let diag_s = Instant::now();\n        if Instant::now() >= next_status || stopping {\n")
rep("        if let (Some(o), Some(w)) = (owner.as_mut(), &checkpoints) {\n            let due = stopping",
    "        diag_t(\"status\", diag_s);\n        let diag_k = Instant::now();\n        if let (Some(o), Some(w)) = (owner.as_mut(), &checkpoints) {\n            let due = stopping")
rep("        // BSC3-016 review L5: a finished census reaches the status at once.\n",
    "        diag_t(\"checkpoint\", diag_k);\n        let diag_n = Instant::now();\n        // BSC3-016 review L5: a finished census reaches the status at once.\n")
rep("        // Phase E review L3: the Four drift guard on its own thread;",
    "        diag_t(\"census\", diag_n);\n        let diag_f = Instant::now();\n        // Phase E review L3: the Four drift guard on its own thread;")
rep("        if stopping && adapters.is_empty() {\n            drop(ingress);",
    "        diag_t(\"four\", diag_f);\n        if stopping && adapters.is_empty() {\n            drop(ingress);")
s += '''
fn diag_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn diag_t(name: &str, t: Instant) {
    let d = t.elapsed();
    if d >= Duration::from_millis(1) {
        eprintln!("DIAG {} {name} {}us", diag_ms(), d.as_micros());
    }
}
'''
open(p, 'w').write(s)
print("ok")
