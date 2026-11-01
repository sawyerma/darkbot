# Configuration examples

Public, secret-free examples. Nothing here is loaded automatically; copy a file, adjust it and pass it explicitly.
Every example is parsed by the real parser in the test suite (`runtime_config.rs` test
`config_examples_parse_with_the_real_parser` and `tests/python/test_run_layout.py` `ConfigExamples`).

| File | Use | Parser |
|---|---|---|
| `operational_override.conf` | `--config FILE` / `P0_OPERATIONAL_CONFIG_FILE`: profile plus selected overrides | Rust `runtime_config.rs` (native text) |
| `operational_inherit_endpoints.conf` | override file whose empty endpoint values take the provider registry's selection (DEC-18; or a named loopback `P0_RPC_URL` / `P0_WS_URL`) | Rust `runtime_config.rs` |
| `provider_qualification.example.json` | shape of `--provider-qualification FILE` for the pinned legacy Python index/crosscheck verification (JFI-03, offline only; not part of the native run); deliberately **not passing** | Python `measurement_contract.preflight` |

Rules:
- Config files are native text: `key = value` per line, `#` comments, CLI names and values; they contain only
  overridden fields, unknown or duplicate keys are rejected. Precedence:
  profile < environment < config file < CLI. Profiles themselves are in [`../profiles/`](../profiles/).
- DEC-18: a remote RPC/WSS provider is the provider registry's (`p0ctl provider add`, `select`), its key the macOS
  keychain's; a named endpoint (environment, file, CLI) is a loopback one only, and an endpoint URL with a key in its user
  info, query, fragment or path is refused. Keypairs or wallet secrets never go into a versioned file.
- No configuration enables LIVE or submission; FREE_ONLY stays binding. A provider qualification passes only with measured,
  recent evidence (`passed: true`, `evidence_sha256`, quota at least the requested budgets).
- The environment template is [`../../.env.example`](../../.env.example) (project root, kept there for the existing tooling);
  the public signer identity is [`../wallet.public.env`](../wallet.public.env).
