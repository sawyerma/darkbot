# Strategy C Four d1 (A4, A1), 05.11.2026

- Builds (`bsc_runtime` release, `--features runtime`): before = `85073528` (Four c), after = A4 + A1 on it.
- Replay identity: `strategy-replay` of H `1793049942243`, P8 `1793074421330`/`1793062591241`, G0.4 `1793088319858`/`1793088689212` and the Four run `1793889940032` (root: `git archive 85073528` config, gas, fixtures): text output byte-identical, decoded records identical but file hash and observation time (here G0.4 `1793088689212`, 2 209 208 records). No real position was held in those runs (no `Superseded`).
- Whole replay (G0.4 `1793088689212`, 3 runs each): before 4.744 s median (4.698–5.287), after 4.905 s (4.843–4.922): the journal read dominates, no change measurable there.
- A1 block end (`end_of_block_cost_with_many_observed_tokens`, release, `--ignored`, 400 observed tokens with 40 buyers each, 2000 blocks; 3 runs each): before 59 891–63 853 ns per block (every token cloned each block), after 4 229–4 438 ns (only the tokens the block acts on are taken): about 14x less.
- A4: a real position (`Hold`, `Reheld`) taking a PAPER one's place emits `Superseded` (strategy record `Y` kind 7, status `strategy.superseded`, runtime-status v30); a position held again keeps its exit trigger. Mutants `tools/mut_sc4d1.py` 8/8 RED.
