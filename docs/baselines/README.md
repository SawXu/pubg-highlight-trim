# #66 frozen behavior and local evidence

Source reference: `a070076526f918b0ef7f1848994d69363d5c1291`. Fixtures contain
synthetic names or existing public crops, never new private recordings/screenshots.

- `issue66_cli_contract.json`: parser cases checked against the detached old
  parser; exact CSV/candidate column order and JSON field types captured from
  its completed source-built-engine run. There are no invented `Action`,
  `Subject`, `Source`, `StartSec` or `EndSec` columns in the detection CSV.
- `issue66_events.json`: 27 synthetic texts re-executed against the detached
  Python parser, preserving every actual event field. Unsupported `mixed` is
  not a language profile: a mixed-script case uses explicit `zh-Hant`, exactly
  as the application does. Old edge behavior is frozen, not silently improved.
- [Canonical planning fixtures](../../rust/ocr_engine/fixtures/planning.json):
  the 45 real planning goldens and Rust replay tests are the only planning
  oracle. Their original capture commit `75916e2` differs from `a070076` only
  in OCR language-cache field naming outside all planning functions. The
  separate provisional planning schema/handwritten expectations were removed,
  not misrepresented as a historical capture. Additional 1,500 old/new planning
  comparisons and 1,540 frozen schedules protect floating-point boundaries.
- [Audited real ROI reference](../../rust/ocr_engine/fixtures/real/README.md):
  18 image/model hash-bound references with actual old OCR and event results.

## Private local regression

Configure a local evidence root; repository commands/CI never depend on an
operator's absolute path. Private manifests, names, hashes, logs and media remain
there, not in GitHub. Inputs are read-only; reports and synthetic/temporary media
are written outside the original corpus. Archive tests use temporary copies only.

The inventory separates 24 root recordings, 79 nested staging/other files and
3,147 historical/output candidates (including seven generated trim clips).
Conservative regression selection includes all 24 root inputs plus 71 additional
staging inputs after removing one identical root/staging SHA-256 duplicate:
**95 candidates**. Staging includes re-encoded comparison inputs rather than
assuming that an unfamiliar nested directory can be excluded. Explicit replay
inputs are retained as negative/bypass-contract cases. Historical outputs are not
accidentally recursively scanned as originals.

An old `offline-final/ocr_engine.exe` artifact was found to predate the source's
word-order fix. Its 1,340-second/10-included run is **not a valid current-source
baseline**. The detached reference engine was rebuilt from `a070076` and its
SHA-256 recorded locally and in the public ROI reference. The corrected root
23 auto-selected sources completed with exit 0: 2 included, 21 skipped,
0 unreadable. The first Rust root comparison matched all 31 nonvolatile semantic
and telemetry columns exactly, but lacked a reliable native exit-code capture;
it remains observational, not final acceptance.

Full 95-input old/new comparisons, one/automatic/explicit worker evidence,
source-integrity checks and audited v3 process-tree resource measurements are now
recorded in the [final verification ledger](../rust-migration-verification.md).
That ledger distinguishes original and upstream-deduplicated corpus counts,
preserves the failed zero-growth elapsed gate alongside explicit user acceptance
of the measured differences, and links successful clean no-Python Windows
CLI/GUI archive execution plus two-axis review. No arbitrary percentage was
introduced; concurrent-load measurements remain observational and known old
defects remain separate. Latest full Rust CI is pending, not inferred from
release success; consult the ledger for the current run identity and outcome.
