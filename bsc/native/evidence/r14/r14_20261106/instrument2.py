import sys
p = sys.argv[1]
s = open(p).read()
def rep(old, new):
    global s
    assert s.count(old) == 1, old[:70]
    s = s.replace(old, new)
rep("            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());\n            let now = clock.now_ns();\n",
    "            let diag_p = Instant::now();\n            journal_drops(&mut journal, &dropped, &mut journaled_drops, clock.now_ns());\n            diag_t(\"status.drops\", diag_p);\n            let diag_p = Instant::now();\n            let now = clock.now_ns();\n")
rep("            let observed = observed_now()?;\n            let bytes = status_bytes(\n",
    "            diag_t(\"status.stats+signals\", diag_p);\n            let diag_p = Instant::now();\n            let observed = observed_now()?;\n            diag_t(\"status.observed_now\", diag_p);\n            let diag_p = Instant::now();\n            let bytes = status_bytes(\n")
rep("            replace_volatile(&status_path, &bytes)?;\n        }\n",
    "            diag_t(\"status.bytes\", diag_p);\n            let diag_p = Instant::now();\n            replace_volatile(&status_path, &bytes)?;\n            diag_t(\"status.replace\", diag_p);\n            eprintln!(\"DIAGLEN {}\", bytes.len());\n        }\n")
open(p, "w").write(s)
print("ok")
