//! BSC-050 A-scan B6: no evidence writer turns a clock error into the
//! valid-looking timestamp 0 (1970); they all use the record crate's
//! `observed_now`, which refuses it.

#[test]
fn no_provenance_clock_falls_back_to_zero() {
    for (name, source) in [
        (
            "g06a_self_launch_bundle.rs",
            include_str!("../src/bin/g06a_self_launch_bundle.rs"),
        ),
        (
            "g06a_requal_check.rs",
            include_str!("../src/bin/g06a_requal_check.rs"),
        ),
        (
            "g06a_native_bnb_quote.rs",
            include_str!("../src/bin/g06a_native_bnb_quote.rs"),
        ),
        (
            "g07_candidate/main.rs",
            include_str!("../../execution/src/bin/g07_candidate/main.rs"),
        ),
    ] {
        let code = &source[..source.find(concat!("#[cfg(test)]\n", "mod tests")).unwrap_or(source.len())];
        assert!(
            !code.contains("UNIX_EPOCH"),
            "{name} reads the clock itself"
        );
        assert!(
            code.contains("observed_now()"),
            "{name} does not use observed_now"
        );
    }
}
