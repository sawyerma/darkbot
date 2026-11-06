import sys
p = sys.argv[1]
s = open(p).read()
def rep(old, new):
    global s
    assert s.count(old) == 1, old[:80]
    s = s.replace(old, new)
rep("            let diag_ev = Instant::now();\n",
    "            let diag_ev = Instant::now();\n            let diag_cpu = diag_cpu_ns();\n            let diag_kind = match &event { Event::Full { .. } => \"full\", Event::HashOnly { .. } => \"hash\", Event::Rejected => \"rej\", Event::Alive => \"alive\", Event::Connected => \"conn\", Event::Disconnected { .. } => \"disc\" };\n")
rep("            let decided = core.observe(source, observation, at, wall.wall_us(at));\n",
    "            let decided = core.observe(source, observation, at, wall.wall_us(at));\n            let diag_o = diag_ev.elapsed().as_micros();\n            let diag_launch = decided.is_some();\n")
rep("            }\n            // Plan 28.10.: on the LIVE path when it is armed,",
    "            }\n            let diag_c2 = diag_ev.elapsed().as_micros();\n            // Plan 28.10.: on the LIVE path when it is armed,")
rep("            match decided {\n                Some(Launch::First { hash, source, .. }) => {",
    "            let diag_x = diag_ev.elapsed().as_micros();\n            match decided {\n                Some(Launch::First { hash, source, .. }) => {")
rep("            diag_t(\"event\", diag_ev);\n",
    "            let diag_w = diag_ev.elapsed().as_micros();\n            if diag_w >= 1000 {\n                eprintln!(\"DIAGEV {} {} wall={diag_w} cpu={} observe={diag_o} c2={diag_c2} exec={diag_x} {diag_kind} launch={diag_launch}\", diag_ms(), source.name(), (diag_cpu_ns() - diag_cpu) / 1000);\n            }\n")
s += '''
#[repr(C)]
struct DiagTs {
    s: i64,
    ns: i64,
}

unsafe extern "C" {
    fn clock_gettime(id: u32, ts: *mut DiagTs) -> i32;
}

/// This thread's CPU time (macOS CLOCK_THREAD_CPUTIME_ID = 16).
fn diag_cpu_ns() -> u64 {
    let mut t = DiagTs { s: 0, ns: 0 };
    unsafe { clock_gettime(16, &mut t) };
    (t.s as u64) * 1_000_000_000 + t.ns as u64
}
'''
open(p, "w").write(s)
print("ok3")
