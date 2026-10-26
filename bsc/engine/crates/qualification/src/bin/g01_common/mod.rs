//! Code shared by the read-only G0.1 freeze binaries (`g01_four_freeze`,
//! `g01_flap_freeze`): the JSON edge (JSON-RPC, Sourcify, archived legacy
//! files), canonical header chains, runtime-code facts and CREATE2, Sourcify
//! lookups and command-line helpers. Each binary uses a subset.
#![allow(dead_code)]

pub mod bytecode;
pub mod chain;
pub mod edge;
pub mod sourcify;
pub mod util;

#[cfg(test)]
mod tests {
    /// JSON exists only in `edge`; nothing public of it names a JSON type.
    #[test]
    fn json_stays_inside_the_common_edge() {
        for (name, source) in [
            ("bytecode", include_str!("bytecode.rs")),
            ("chain", include_str!("chain.rs")),
            ("sourcify", include_str!("sourcify.rs")),
            ("util", include_str!("util.rs")),
            ("mod", include_str!("mod.rs")),
        ] {
            assert!(!source.contains(&["serde", "_json"].concat()), "{name}");
            assert!(!source.contains(&["json", "!"].concat()), "{name}");
        }
        let edge = include_str!("edge.rs");
        for line in edge.lines() {
            if line.trim_start().starts_with("pub ") {
                assert!(!line.contains("Json"), "{line}");
            }
        }
        assert!(edge.contains("rpc_wire::post("));
        for argv in ["\"--data\"", "\"--data-binary\"", "\"-H\""] {
            assert!(!edge.contains(argv));
        }
    }
}
