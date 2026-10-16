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

## Gate P1B

Tickets P1B-01..03 done. Upgrade safety: an upgrade halts and no 0.1.12 bank is misread by 0.1.11 code. Not yet shown: a
trial build of DarkBot against the 0.1.12 crates (the promotion step 3/4 itself) — NOT_RUN; it needs the released tag that
is actually deployed.
