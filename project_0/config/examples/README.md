# Configuration examples

Public, secret-free examples. Nothing here is loaded automatically; copy a file, adjust it and pass it explicitly.
Every example is parsed by the real parser in the test suite (`runtime_config.rs` test
`config_examples_parse_with_the_real_parser` and `scripts/test_run_layout.py` `ConfigExamples`).

| File | Use | Parser |
|---|---|---|
| `operational_override.json` | `--config FILE` / `P0_OPERATIONAL_CONFIG_FILE`: profile plus selected overrides | Rust `runtime_config.rs` (`Project0/OperationalConfig/1`) |
| `operational_inherit_endpoints.json` | override file whose empty endpoint arrays inherit `P0_RPC_URL` / `P0_WS_URL` from the local environment | Rust `runtime_config.rs` |
| `provider_qualification.example.json` | shape of `--provider-qualification FILE` for FREE_ONLY verification; deliberately **not passing** | Python `measurement_contract.preflight` |

Rules:
- Config files contain only overridden fields; unknown keys, comments and `$schema` are rejected. Precedence:
  profile < environment < config file < CLI. Profiles themselves are in [`../profiles/`](../profiles/).
- Never put endpoint URLs with keys, keypairs or wallet secrets into a versioned file. A file with endpoints overrides the
  environment; keep such files outside Git with owner-only permissions.
- No configuration enables LIVE or submission; FREE_ONLY stays binding. A provider qualification passes only with measured,
  recent evidence (`passed: true`, `evidence_sha256`, quota at least the requested budgets).
- The environment template is [`../../.env.example`](../../.env.example) (project root, kept there for the existing tooling);
  the public signer identity is [`../wallet.public.env`](../wallet.public.env).
