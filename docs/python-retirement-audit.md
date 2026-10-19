# #63 Python retirement audit

## Final status

Project-owned Python application, tests, scripts and packaging entry points are
retired; official replacements use Rust, PowerShell and WPF/.NET, without Python
or Paddle fallback. Full local 95-source comparisons, audited resource evidence,
actual Python-free Windows CLI/GUI ZIP execution and independent two-axis review
are now recorded in the [final verification ledger](rust-migration-verification.md).
The user accepts the measured elapsed differences; the raw zero-growth time gate
remains false. **Latest Rust CI run 39186555704 / `53fce44` is pending**, not an
inferred pass. The ledger is authoritative for current acceptance, CI identities,
all 22 stories, implementation/testing decisions and remaining administration.

The inventory and execution records below preserve their historical audit
coordinates: retirement base `3835eed`, initial guarantee audit at `038e51d`, and
reconciliation at `738a65d`. References to worktree-local results describe those
stages, not current missing coverage or a new final-test total. The historical
111-test run is not substituted for the latest full Rust CI. This audit operation
did not access private recordings; subsequent local corpus validation is summarized
without private identities in the ledger. Git history and vendored upstream code
remain untouched. Nothing in this audit itself merges PR #72 or publishes a tag.

## Application and packaging inventory

All paths below refer to the retired base tree. Rust paths are relative to `rust/`.

| Retired project-owned Python | Active replacement / preserved guarantee |
|---|---|
| `src/pubg_highlight_trim/{__init__,__main__,cli}.py`, `pyproject.toml` | `ocr_engine/src/{config,application}.rs`, `bin/pubg_highlight_trim.rs`; CLI defaults/aliases/validation/exit/version, production Cargo binary. No Python install/package entry point remains. |
| `game_languages.py`, `game_language_tables.py` | `events.rs`, `source_files.rs`; explicit three-language parsing, filename language inference, target selection and exclusion. |
| `ocr.py` | `events.rs`, `scanner.rs`, `scan.rs`, `frame.rs`, `scan_adapter.rs`; parser, coarse/adaptive/refine/assist, budgets, shared frames, preprocessing and lifecycle. |
| `frame_source.py`, `ffmpeg_tools.py` | `media.rs`; software per-seek default, explicit hardware fallback, BGR frame shape, unreadable versus failed frame, timeout, trim/concat and source alias rejection. Python OpenCV capture internals are retired, not a supported second implementation. |
| `cache.py` | `persistent_cache.rs`; new namespace/full identity, completion/checksum, corruption miss and atomic write. Old Python cache ABI intentionally unsupported. |
| `pipeline.py`, `models.py`, `source_files.py` | `application.rs`, `reports.rs`, `output_files.rs`, `source_files.rs`, `planning.rs`; discovery, candidates, bounded workers, stable ordering, outputs/reports/archive safety. |
| `planning.py` | direct canonical `planning::plan_clips`; 45 frozen production cases. Temporary serialization bridge is removed. |
| `rust_ocr.py`, `runtime.py` | direct Rust OCR library; no Python-to-Paddle result reshaping, multiprocessing frozen startup, native Python descriptor suppression or Python encoding environment is needed. Stable UTF-8 stdout/stderr and historical report labels remain. |

`ocr_engine --plan-clips` is removed. All single JSON/JSONL ordering, CRLF,
prior-complete-response-on-error, invalid/empty request, float precision and clean
stdout checks now use canonical `highlight_plan` or its same planner module.
`highlight_plan` is a developer fixed-input diagnostic, **not a model-free video
preview** and not evidence of OCR/model quality.

WPF `CliRunner` keeps UTF-8 readers and process-tree cancellation, configures only
the bundled model directory, and no longer injects Python or backend/engine bridge
environment variables. No OCR diagnostic executable is required to discover models.

## Retired executable test inventory

These are guarantee mappings, not a claim that matching internal test names prove
full corpus parity. Language-neutral JSON/JSONL, pixel fixtures and already-public
ROI images are retained in `rust/ocr_engine/fixtures`, `rust/ocr_engine/tests/fixtures`
and `tests/fixtures/missed_event_ocr`.

| Retired Python test file | Current executable coverage / intentional retirement |
|---|---|
| `test_cli_defaults.py` | `pubg_highlight_trim_cli.rs`, `application_contract.rs`: defaults/options/aliases/files/merge/jobs/invalid flags/help/version and UTF-8 paths. |
| `test_cli_logging.py` | production CLI tests, `end_to_end.rs`, WPF output parser tests; lifecycle/progress/summary/errors and independent verbose/profile. Python traceback/frozen-worker suppression are obsolete implementation details, not removed machine protocol guarantees. |
| `test_game_languages.py` | `events_contract.rs` ports concrete key/action/weapon/fall/assist/delayed/Molotov cases; `application_files.rs` covers filenames/language votes; invalid language is rejected. Python regex-table object identity is no longer relevant. |
| `test_ocr_refine.py` | `scanner.rs` 33 fixed backend/clock scenarios, event contract and fixed preprocessing tests: multiple subjects, same/different assists, suffix/headshot/noisy/follow-up suppression, backward refinement, nonrecursive adaptive expansion, shared frame/OCR caches. |
| `test_performance_path.py` | scanner/scan fixtures, CLI config validation, `persistent_cache.rs`, `application_reports.rs`: budgets, ROI, candidate anchors/boundaries, cache completion/config identity and candidate CSV schema. |
| `test_frame_source.py` | actual FFmpeg `media.rs`: H264/AV1 software seek, hardware fallback, missing/bad/timeout/single-frame classification, close lifecycle. Original synthetic AV1 median <= 4 * H264 smoke bound is now an actual Rust test (same five seek times), not relaxed. |
| `test_ffmpeg_tools.py` | media discovery and invalid explicit paths; PowerShell bundle smoke executes actual bundled FFmpeg/FFprobe. |
| `test_pipeline_merge.py` | planning 45 cases, application file/report/CLI tests, actual media trim/concat and real-model end-to-end: split/no merge, source conflicts, unique/overwrite names, sorted files, candidate/full decisions, move-original safety using temporary copies. |
| `test_source_files.py` | `application_files.rs`: target/language/timestamp/recursion, explicit source handling, ambiguous language and exclusions; final 95-source coverage is recorded in the verification ledger. |
| `test_unreadable_reporting.py` | scanner explicit source/model/gate failures, actual bad FFmpeg media, production error/summary and end-to-end batch paths; failures cannot become clean no-event scans. |
| `test_missed_event_ocr.py` | feature-gated real-model `preprocess.rs::real_roi_preprocessing_keeps_recognized_subjects` uses the same three public images and concrete event/subject assertions. It remains an explicit real-model gate. |
| `test_ocr_backend_defaults.py`, `test_rust_ocr_backend.py` | Rust engine provider/status/inference-error tests and CLI/runtime smoke; direct Rust app has no Python/Paddle adapter, import fallback or Python process exit mapping. Report historical Paddle field/label compatibility remains covered. |
| `test_runtime.py` | Python encoding/reconfiguration/descriptor machinery is retired; Rust CLI UTF-8 and WPF UTF-8 stdout+stderr/cancellation are executed instead. |
| `test_ocr_model_manifest.py` | `fixture_contract.rs` controlled HTTPS pinned source/hash/languages, xtask verify corruption/missing/dictionary, HTTP hash-mismatch download test preserving existing asset/no `.part`. Full ONNX assurance uses mandatory ORT model load and real detection/recognition (see final developer-tool replacement). |
| `test_real_ocr_benchmark.py` | `fixture_contract.rs`: 18 public image identities and audited reference languages (8 Hans / 4 Hant / 6 English), both event perspectives per language, actions and nonnegative historical timestamps. `reference.json`, not the inaccurate historical annotation labels, is the actual event oracle; see audited ROI gate below. |
| `test_compare_ocr_baselines.py` | xtask CLI and `retirement_contract.rs`: whitespace normalization, semantic mismatch/nonzero, box ordering, missing frame and mean inference-time delta. |
| `test_audit_ocr_bundle.py` | xtask actual filesystem/ZIP audit; Python runtime which old audit allowed is now forbidden. Nested Paddle module and malformed ZIP reject paths are executed. |
| `test_build_supply_manifest.py` | xtask stable consecutive manifest, self-exclusion, SHA256/byte count and actual components. CPython/NumPy/OpenCV/PyInstaller version assertions are intentionally obsolete because those components may not ship. |
| `test_prune_windows_bundle.py` | obsolete package-pruning responsibility retired; see below. Safety does not depend on silently deleting invalid artifacts. |
| `test_windows_packaging_scripts.py`, `test_windows_ui_bundle.py` | stale Python/OpenCV/PyInstaller source-string expectations removed. Actual PowerShell final bundle/ZIP smoke, DPI/PE/UI verification and WPF contracts are preserved. Successful final production release execution is linked in the verification ledger, separately from pending latest Rust CI. |

Two real migration bugs were found by porting old exact assertions, not weakening
them: English `KILLED` was incorrectly treated as a kill-count token without `\b`,
breaking assist exclusion; English self weapon lookup searched before the action
instead of its trailing `WITH/USING`. Minimal fixes retain old expected results.
Additional Rust source-selection cases preserve the old concrete three-language
filename sets and out-of-target language-voting behavior. Manifest nested Windows
paths are normalized to `/` and verified, preserving the original tool contract.

## Tools: replacement and justified retirement

| Retired script | Rust/PowerShell responsibility |
|---|---|
| `download_ocr_models.py` | `xtask download`: pinned SHA256, dictionary extraction, cache hit and atomic replacement. Negative HTTP fixture asserts hash failure preserves existing asset and creates no partial file. |
| `extract_ppocr_dictionary.py` | `xtask dictionary`: structured YAML extraction, optional expected hash, atomic write. |
| `build_supply_manifest.py` | `xtask manifest`: stable manifest excluding itself, actual file SHA256/bytes/components/sources/licenses. |
| `audit_ocr_bundle.py` | `xtask audit` plus PowerShell runtime/archive checks: actual filesystem and ZIP entry inspection, invalid ZIP failure, no Python/Paddle residue. |
| `report_bundle_size.py` | `xtask size`: actual unpacked/ZIP bytes, largest groups/files. |
| `compare_ocr_baselines.py` | `xtask compare`: text/action/event/target/box equality, missing frame identity, timing delta report. Left/right labels replace Python/Rust implementation names. |
| `run_real_ocr_benchmark.py` | At retirement base, `xtask real-roi` reports public image inference/status/boxes and annotation differences. Integration `91663f4` strengthens this into a required actual-event/reference gate with nonzero failure for target/action/subject/key or inference/protocol regressions; the weak annotation-only behavior must not overwrite that gate. |
| `smoke_ocr_engine.py` | `xtask onnx-smoke` loads both models and recognizes the hash-pinned public ROI with frozen text/event/box assertions. PowerShell's binary-pipe protocol smoke is additionally run under both CP65001 and CP936. |
| `prefetch_ocr_models.py` | **Obsolete:** downloaded Paddle/PaddleX runtime caches; those backends/packages are explicitly unsupported in #63. Pinned ONNX `xtask download` is the only required download path. No Python/Paddle cache prefetch belongs in Rust production. |
| `prune_windows_bundle.py` (and redundant `xtask prune`) | **Obsolete:** cleaned PyInstaller-collected duplicate FFmpeg DLLs and Paddle/hf_xet/Shapely development debris. Explicit Rust bundle assembly does not collect these packages or duplicate DLL trees. Audit must reject prohibited residue, not prune it to manufacture a pass. FFmpeg's one complete shared-runtime tree is retained. |

## Final developer-tool replacement

The four blockers identified in the first retirement preparation were subsequently
replaced by `0919024` and its tested corrections. They are no longer project-owned
Python files; `git ls-files '*.py' '*.pyi'` is empty after integration.

1. `build_real_ocr_benchmark.py` → `xtask build-real-roi`: CSV/BOM selection,
   language/perspective and assist/delayed preference, original-source lookup,
   local seek/ROI, JPEG/baseline/manifest generation. Nonempty output directories
   are rejected. The executable test uses synthetic media from a public ROI,
   not private videos or uploads.
2. `bench_frame_read.py` → `xtask frame-bench`: the actual old coarse, backward
   refine and assist sequence, per-variant warmup, fastest complete repeat and
   total/mean/median/p90. Its CLI test checks the complete observed seek order,
   not just a coarse subset.
3. `verify_ocr_models.py` → `xtask verify --check-onnx --engine <exe>`: hashes,
   unique nonempty UTF-8 dictionary symbols and graph/IO metadata are checked.
   Protobuf decoding alone is explicitly insufficient: the option requires the
   supplied engine to load the complete models in ORT and execute real detection
   **and recognition**, checking the frozen ROI result. This is the equivalent
   full-load/real-inference assurance allowed by #63, not a claim to reimplement
   every ONNX checker diagnostic.
4. `onnx_cpu_smoke.py` → `xtask onnx-smoke`: no blank-image shortcut. It reads a
   hash-pinned public real ROI, asserts actual recognized text, event identity,
   nonempty valid boxes and CPU initialization; failures/status-only replies and
   hangs fail, with timeout kill/reap. Both model sessions must load and inference
   must produce the expected text to pass.

Actual controlled-model smoke and the audited 18-frame semantic gate passed.
`retirement_contract` additionally checks a real pinned HTTPS response with an
intentionally wrong SHA: the old asset survives and no partial file is published.
That network-dependent test is explicitly run by provisioning CI, rather than
making ordinary offline unit tests depend on network access.

## Historical coverage-audit reconciliation (integration `738a65d`)

This section preserves the audit-stage distinctions. The final ledger supersedes
its remaining-execution notes; do not interpret them as current absent coverage.

The initial shared audit correctly identified missing protections at `038e51d`.
Several are now present in committed integration code; they must not be reported
as nonexistent, nor confused with execution on this retirement branch:

| Initial audit concern | Later committed gate inspected / remaining distinction |
|---|---|
| Direct events/source coverage | `source_profiles.rs` now checks all language filename terms, replay/DVR boundaries, excluded-target voting and match-end language. `fixtures/events_golden.json` is committed fixed data; the events worker is integrating its parser execution and the exact old event regressions found here. Data existence alone is not counted as an executed gate. |
| Decimal/random scheduling | `scan_schedules.rs` + `fixtures/scan_schedules.json` are committed on integration, rather than the uncommitted work cited by the early audit. Keep their deterministic schedule assertions. |
| CLI old flags/options | `pubg_highlight_trim_cli.rs::removed_legacy_flags_are_rejected_and_never_advertised` is now committed, complementing original defaults/validation/help tests. |
| Unreadable/error channels | `media_errors.rs`, `media_launch_failure.rs`, `media_output.rs` and typed media/runtime errors are committed; `application_e2e.rs` includes blank+unreadable batch exits, failed cache-hit probe, provider fallback and missing models. Model-dependent tests are explicit ignored acceptance gates, not nonexistent tests or evidence that they ran here. |
| Application black-box pipeline | `application_e2e.rs` now includes actual own-kill scan/trim/merge/report/cache, split/archive temporary sources, candidate auto/forced-full, source-order/parallel equivalence and console cancellation. `cancellation.rs` also checks cancellation handling. These are complementary to the earlier public ROI video end-to-end; enabled real-model/Windows execution evidence must be recorded by the coordinator. |
| Tool/archive protections | Retirement adds atomic bad-download, timing/missing-frame, nested ZIP/hash/manifest tests. Integration has additional assurance metadata changes. Full final archive/clean-host acceptance remains separate; all four formerly retained developer tools now have executable Rust replacements. |

### Audited public ROI gate (do not regress to annotation equality)

Integration `91663f4` supplies the audited manifest and `reference.json`, frozen
from the exact `a070076` compiled engine plus original Python event parser.
The coordinator reports the real-model 18-frame actual-event gate passed 100%.
This evidence is distinct from this worktree's local fixed-fixture tests; it is
not a complete private-video or performance acceptance.

The audited image languages are **8 `zh-Hans`, 4 `zh-Hant`, 6 `en`**. Images 10/12
are simplified Chinese even though historical `baseline.jsonl` labels said Hant.
Some historical annotations combined notifications or mislabeled assist/delayed
meaning. Preserve them as historical material, not truth; use frozen actual
version-2 event-field fingerprints and per-image hashes (not repeated player identifiers). The prior retirement metadata change to
six each was incorrect and is superseded by the copied audited README/manifest.
The static `fixture_contract.rs` now validates reference identity and audited
coverage; it does not claim to run inference. Keep integration's `xtask real-roi`
actual production parser, hash, semantic-failure exit and per-frame evidence.

## Official call inventory and acceptance gates

- `.github/workflows/{ocr-rust,release}.yml` use Cargo, PowerShell and .NET only;
  Python-name checks/remove-preinstalled steps are enforcement, not Python calls.
- `scripts/{build_windows,build_ui_windows,smoke_windows_bundle,verify_windows_ui_bundle,verify_windows_dpi,download_ffmpeg}.ps1`
  do not call Python. Existing package/DPI/hash/read-only/binary-pipe checks remain.
- README bilingual development/build instructions now match actual Rust/.NET tools.
  Production notes separate current structure from historical OCR-only facts.
  Historical `docs/releases/*`, past measurements and old regression evidence are
  not rewritten to pretend historical releases were Python-free.
- No `pyproject.toml`, Python app/source/test runner or temporary bridge remains.
  All formerly blocked developer tools have also been replaced. Upstream vendor
  files and Git history are explicitly outside retirement scope.

Recorded local commands (this worktree; not a Python-free clean-machine claim):

- `cargo test --locked --manifest-path rust/Cargo.toml --workspace --all-features`
  — 111 passed, 3 explicitly ignored real-model tests, zero failed; actual generated
  H264/AV1 seek/trim/merge and the original 4x timing smoke all ran.
- Focused event contract: 3 passed after preserving/fixing exact old expectations.
  Canonical planner: 9 passed; xtask: 7 passed; fixture contract: 2 passed (included
  in the 111 total, not additional tests).
- `dotnet test ui/PubgHighlightTrim.Ui.Tests/PubgHighlightTrim.Ui.Tests.csproj --configuration Release --nologo`
  — 27 passed, zero skipped/failed, including UTF-8 stderr/stdout, model lookup
  without bridge executable and actual process cancellation.

## Final acceptance reconciliation

The former outstanding list above is no longer the current state: complete
95-source single/automatic/explicit comparisons, final quiet v3 creation-time and
parent-chain memory audits, unchanged source SHA256 values, explicit user
acceptance of +0.7496248 s / +3.2573687 s elapsed differences, clean no-Python
Windows release, real final-ZIP GUI completion, offline public OCR-to-trim,
397 notice hashes and Standards/Spec review now have evidence. See the
[final verification ledger](rust-migration-verification.md) for exact numbers,
upstream cross-source-dedup count reconciliation, additive `ocr_providers`,
artifact sizes and public run links. Earlier invalid memory sampling is not used.

The latest Rust test/assurance run **39186555704 / `53fce44` remains pending**.
Its executed ignored/model/media/CLI/license results, or those of the superseding
final documentation head, must be recorded after remote completion in the final
PR #72/tracker addendum linked from the ledger. PR checks determine the final-head
status; release success and historical unit counts do not replace that gate.
Tracker closure, PR readiness and worker cleanup are coordinator actions, not
implied by this inventory. No private source names, paths or hashes are published.
