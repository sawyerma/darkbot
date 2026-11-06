import sys
# R14 M3 diagnosis (scratch copy only): each canonical event's apply time from 1 ms (runtime_canonical.rs drain_canonical).
p = sys.argv[1]
s = open(p).read()
old = """    while let Ok((at, event)) = crx.try_recv() {
        apply_canonical(c, owner, exec, live, journal, clock, at, event);
    }
}
"""
assert s.count(old) == 1
new = """    while let Ok((at, event)) = crx.try_recv() {
        let kind = format!("{:?}", std::mem::discriminant(&event));
        let t = std::time::Instant::now();
        apply_canonical(c, owner, exec, live, journal, clock, at, event);
        let d = t.elapsed();
        if d >= std::time::Duration::from_millis(1) {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            eprintln!("DIAG {ms} apply_canonical {kind} {}us", d.as_micros());
        }
    }
}
"""
open(p, "w").write(s.replace(old, new))
