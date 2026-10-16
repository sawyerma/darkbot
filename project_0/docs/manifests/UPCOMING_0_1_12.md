# UPCOMING_0_1_12 — P1B freeze (2026-10-16)

Status: `mrgn-0.1.12-rc1` is a pre-release. Not LIVE before the on-chain deployment is confirmed. The binding change list is
PHASENPLAN P1B; this manifest records what DarkBot does with it. Layout values cross-checked against the local 0.1.12
source checkout `0dotxyz/marginfi-v2` master `7d48d8a` (2026-10-02, program crate 0.1.12): `assert_struct_size!(Bank, 3904)`,
`Bank::V1_LEN = 1856` ("byte-identical prefix of the current layout"), `FeeState::V1_LEN` with the premium wallet behind it.

## P1B-01 — Changes and DarkBot handling under the 0.1.11 pin

| 0.1.12 change | Handling now | After promotion |
|---|---|---|
| Bank resize 1856 → 3904 B (payload) | `protocol::decode_bank` accepts exactly `8 + size_of::<Bank>()` of the pinned crate; a 3904 B bank is `UNKNOWN_LAYOUT: bank layout 0.1.12` in Health observation, dependency discovery, sizing, builder, Shadow helpers and fixtures. No prefix decode. | the crate bump makes 3904 B the decodable size; a non-resized 1856 B bank becomes `UNKNOWN_LAYOUT: bank layout 0.1.11` → every TX touching it fails closed, as the program does |
| Variable borrow premium (`Balance.premium_rate_snapshot`, `premium_outstanding`, `PREMIUM_ACTIVE` per bank) | premium state lives behind the 1856 B prefix → unreachable, see row above; `premium_decay_per_second` = 0 is correct for 0.1.11 banks only | Health from the 0.1.12 crate (projected premium debt) |
| Liquidation tagging `marginfi_account_tag_liq_record`, `MarginfiAccount.liquidation_tagged_at`, premium curve (5 % / 1 h, linear, 100 % at 7 d, no growth on bad debt or `EMODE_BOOSTED`) | curve and inverse already in `economics.rs`; no tag instruction is built | phase T |
| Receivership health: drop ≤ unweighted `seized − repaid`; solvent account must not end in bad debt | not enforced by 0.1.11 program | sizing checks from the 0.1.12 crate (P4A/P5) |
| `OracleSetup::ScopeKamino = 27`, `ScopeJuplend = 28` | not representable in 0.1.11 types; unknown discriminant cannot decode | `bank_observation_keys` arms from the 0.1.12 crate |
| `MAX_COSTLY_POSITIONS = 4`, new premium fields in Bank/Group/FeeState | not visible to 0.1.11 types | from the 0.1.12 crate |

## P1B-02 — Dual-size Bank decoder tests

- `protocol::tests::dual_size_bank_decoder_fails_closed`: 1864 B raw decodes; 3912 B → `OtherVersion("0.1.12")`; ±8 B around
  both sizes and empty → `Length`; wrong discriminator → `Discriminator`.
- `health::bank_layout_tests::resized_bank_fails_health_observation_closed`: a MarginfiAccount with one active balance on a
  resized bank → Health observation `Unsupported(UNKNOWN_LAYOUT …)`, no dependency keys.

## P1B-03 — Promotion procedure (after the actual Mainnet deployment)

1. **HALT** — automatic: the ProgramData change latches `PROGRAM_UPGRADE` at runtime; a restart on the new deployment stays
   halted because `deployment_pin` ≠ `Reference`. The HALT log names the new deployment (slot, authority, hash).
2. **ProgramData / program hash** — authorized finalized read of program + ProgramData; record slot, upgrade authority, hash.
3. **Source** — identify the release tag of the deployment; bump every `mrgn-0.1.11-rc1` git pin in
   `native/crates/liquidation/Cargo.toml` and `native/crates/runtime/Cargo.toml` to it and update `SOURCE_TAG`, `SOURCE_COMMIT`, `PROGRAM_DATA_REFERENCE_HASH`.
4. **Layout / IDL** — `source_pin_parity_with_type_crate` and `dual_size_bank_decoder_fails_closed` fail until
   discriminators, `BANK_PAYLOAD_LEN_*` assertions and the lockfile pin match the new crate; fix, never relax.
5. **Bank resize** — per bank of the group: `decode_bank` → decodable or `OtherVersion("0.1.11")` (not resized → excluded).
6. **Parity** — FE-01 observation against the new deployment; Health/sizing tests on the new crate.
7. **P1B becomes P1A** — rewrite `CURRENT_MAINNET.md`; LIVE stays disabled until separately re-approved.

## RC2 probe (2026-10-16, isolated scratch copy; productive 0.1.11 pin unchanged)

Releases: `mrgn-0.1.12-rc1` (2026-09-29), `mrgn-0.1.12-rc2` (2026-10-10, commit `9fcb729ae3a6de67544fe6f8147da8d86bdf3692`).
Mainnet deployment of either: not verified (P1A read BLOCKED).

| Check | Result |
|---|---|
| `cargo check` of `darkbot-project0` + `darkbot-runner`, all targets, `crate_health,txv1`, all marginfi pins → `mrgn-0.1.12-rc2` | PASS after 4 call-site changes, nothing else |
| API breaks | `get_health_components` + `premium_scratch: &mut Option<&mut PremiumScratch>` (`health.rs`); `OraclePriceFeedAdapter::try_from_bank` + `ignore_pt_emergency: bool` (`health.rs`, `sizing.rs`); `get_price_and_confidence_and_cache_of_type` + `ignore_pt_emergency: bool` (`health.rs`). Probe used `&mut None` / `false` |
| Promotion tripwires against RC2 | fire as designed: `source_pin_parity_with_type_crate` (Bank 3904 ≠ 1856), `dual_size_bank_decoder_fails_closed` (1864 B no longer decodes), `resized_bank_fails_health_observation_closed` (0.1.11-only expectation) |
| RC2 IDL `marginfi.json` (release asset, 581,271 B, SHA-256 `b4f80a6cecc7ad4828b0cd5f399b34dfd9fbada0fd866b46f3eb3a34cbc72fb2`, version 0.1.12, 124 instructions, 12 accounts) | the 5 account discriminators and all 8 receivership instruction discriminators equal `protocol.rs`; `init_liquidation_record` is named `marginfi_account_init_liq_record` (same bytes); new `marginfi_account_tag_liq_record` `[12,189,215,239,231,122,220,158]`, `marginfi_account_close_liq_record` |

RC2 semantics the promotion must implement (from the 0.1.12 patch notes in the tag):

- Health and liquidation checks add projected premium to liabilities (`premium_scratch`); off-chain Health must pass it.
- `repay` pays premium before principal; `repay_all` (`Option<bool>` arg) pulls principal + premium → repay sizing and
  wallet holds must include premium. Receivership/classic liquidation repayment reduces principal only.
- Any TX touching an unresized bank fails; production banks are resized right after the upgrade (minutes of outage).
- Borrow/end-flashloan with premium-bearing debt need every premium-relevant collateral oracle (`PremiumSnapshotUnavailable` 6615).
- `marginfi_account_close` needs the trailing `rebalance_fee_pool` PDA (not used by receivership).

## Gate P1B: LOCAL_PASS (upgrade-ready offline); deployment BLOCKED

Tickets P1B-01..03 done; DarkBot type-checks against RC2 with a known 4-call migration; an upgrade halts and no 0.1.12
bank is misread by 0.1.11 code. Open until promotion: the premium-aware semantics above (Health `premium_scratch`,
repay/`repay_all` sizing) and which release is deployed on Mainnet (P1A read, permission).
