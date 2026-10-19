# #63 Rust migration verification

Integration PR: #72. This is the final evidence ledger, not permission to merge or
publish a tag. Production migration, full-corpus parity, package execution and
review evidence are recorded below. **Latest Rust CI is pending**; do not treat
configured or skipped tests as executed. Private recordings, crops, filenames,
source hashes and local report paths are not published here.

## Acceptance status and CI identity

| Gate | Evidence / disposition |
|---|---|
| Final production source | Rust application and OCR library, Rust assurance tools, PowerShell packaging and WPF/.NET; no project-owned Python app/tests/scripts or Python fallback. See [retirement inventory](python-retirement-audit.md). |
| Full private corpus | All 95 sources covered in old/automatic/explicit runs; semantic comparisons and original integrity pass. See corpus reconciliation below. |
| Performance | Audited v3 tree RSS passes. Original zero-growth elapsed gate remains **false**; user explicitly accepts this run's +0.7496248 s automatic / +3.2573687 s explicit differences. No new percentage allowance or rewritten raw gate. |
| Final CLI/GUI archives | [Release run 39172092302](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39172092302), head `8de278d41695fc32793fbf93c0044f963dfff1b9`: **success**. Actual clean-host/offline/public-OCR-to-trim/GUI completion evidence below. Later changes through `53fce44` affect only two test files, not production/package code; this production-artifact evidence remains applicable. |
| Latest observed Rust test/assurance run — CI FINAL RESULT | [Run 39186555704](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39186555704), head `53fce44`: **pending at this ledger snapshot**. [PR #72 checks](https://github.com/SawXu/pubg-highlight-trim/pull/72/checks) are authoritative for the final documentation head; coordinator will append the actual executed-gates result in the PR/tracker evidence, not infer success from release CI. |
| Independent Standards + Spec reviews | Findings fixed or individually dispositioned with code/test evidence in [review record](rust-migration-review.md). Existing ordinary issues #73/#74 stay separate. |
| Tracker / PR / worker administration | Evidence does not itself close #66–#71/#63, mark PR #72 ready, merge it, or authorize release. Coordinator records tracker and cleanup completion separately. |

Earlier PR Rust run [39172095833](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39172095833)
failed a Windows short/long-path string assertion in `application_files`; the
following [39173489488](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39173489488)
failed formatting. Neither proves the downstream ignored gates ran. The path
assertion now checks file identity; latest run above is the required replacement.

## Dependency graph and delivered responsibilities

| Ticket | Delivered responsibility | Depends on |
|---|---|---|
| #66 | Frozen old CLI/rule/media fixtures and local original inventory | — |
| #67 | Rust language/event/scan/image/media/planning; production callers switched, old implementations removed | #66 |
| #68 | Rust/PowerShell assurance tools preserve valid old checks; obsolete pruning/Paddle prefetch explicitly retired | — |
| #69 | Production Rust CLI/reports, bounded workers, WPF and source safety | #67 |
| #70 | Python-free official workflows and actual CLI/GUI archive verification | #68, #69 |
| #71 | Full corpus comparison, audited performance, clean Windows artifacts and requirement/review reconciliation; latest Rust CI status above remains authoritative | #66, #70 |

## Full-corpus behavior and safety

The local inventory contains **95 sources, 25.97 GB: 72 H.264 and 23 AV1**
(24 root originals and 71 staging/re-encode candidates). One exact duplicate and
historical generated clips were excluded during inventory, not silently counted
as new source coverage. Source index and stable chronological order disambiguate
repeated basenames. Private identities and reports remain outside the repository.

Two baselines must not be conflated:

- Earlier single-worker comparison against the original migration oracle:
  **67 positive / 28 negative**, **120 report windows: 91 included / 29 skipped**,
  **112 events**, zero unreadable. Main CSV 33 semantic fields, candidate CSV
  25 fields and 25 legacy summary fields match exactly.
- Final quiet v3 comparison includes upstream `50c1177` / #65 / #64 conservative
  cross-source deduplication in both old oracle and Rust: **66 positive / 29
  negative**, **120 windows: 90 included / 30 skipped**, still **112 events**.
  The additional `skipped-cross-source-duplicate` explains the count change; it
  is not an unexplained migration loss. Both Rust automatic and explicit-two
  runs match that final oracle: 120 main rows × 33 semantic fields, 93 candidate
  rows × 25 fields and all 25 legacy summary fields have zero differences.

Comparison excludes run IDs, output locations and measured timing columns, not
business event/time/window fields. Historical `PaddleText`, `PaddleScores`,
method and detector labels remain compatible despite removal of the backend.
`summary.json` intentionally adds **`ocr_providers`**: requested/actual provider,
model version, cache hit, device/fallback status and synthetic 1-based source
index. All 95 final entries report CPU. This is an additive, privacy-reviewed
observability contract, not a legacy-field mismatch. Consequently raw
`full_summary_contract_equal` / all-fields `semantic_pass` remain false while
`csv_semantic_pass` and `legacy_summary_pass` are true; the addition is explicitly
explained rather than deleting it from the raw comparator.

Every v3 run returns native exit **0**, crosschecked with retained Windows process
handle `GetExitCodeProcess` and `Popen.wait`. All 95 original sizes and SHA256
values remain unchanged before/after each run. No original was moved or trimmed
in place. Real output decode/duration/trim/source-integrity behavior is also
executed by public-media CLI/package gates; event/plan comparison is not claimed
to prove encoded bytes are identical.

## Timing, memory and startup (v3)

Same host and ordered quiet runs, warm filesystem/hash pre-read and fresh
process/model sessions; this is **not** a cold-OS/cache-controlled benchmark.
Native creation/exit timestamps, not polling completion latency, define elapsed:

| Measurement | Old automatic | Rust automatic | Rust explicit 2 |
|---|---:|---:|---:|
| Native elapsed seconds | 1107.1630654 | 1107.9126902 | 1110.4204341 |
| Delta versus old automatic, seconds | — | +0.7496248 | +3.2573687 |
| Sampled simultaneous process-tree peak RSS, bytes | 1424461824 | 1379266560 | 1369702400 |
| Launch to first sampled stdout, seconds | 0.2931 | 0.5624 | 0.5595 |

The pre-frozen gate is elapsed and peak tree RSS no greater than the actual old
run, with **no percentage allowance**. Both new time checks and combined
performance gates remain **false**; both RSS checks pass. The user has explicitly
accepted these measured elapsed differences and authorized completion. That is a
recorded decision for this evidence, not a retroactive numerical gate change or
claim of strict performance pass. Explicit-two is a different jobs configuration,
so its comparison to old automatic is not a same-configuration speedup claim.
No unverified wait optimization is included to manufacture a pass.

Memory sampler v3 retains live process handles, creation time and full valid
parent ancestry, rejecting PID reuse. Independent audits recompute exactly the
three RSS values above with **zero violations** (19,683 / 7,495 / 7,489 audited
records). Do not sum individual lifetime peaks: that is not simultaneous tree
RSS. Earlier v2 RSS is invalid and is not acceptance evidence. The old baseline
has completion/independent audit evidence; candidate-specific gate-result files
exist for the two Rust runs, not for the baseline itself.

First-stdout values use **250 ms sampling** and include launch-to-output work;
they are not pure initialization times or evidence of faster startup. Earlier
single-worker elapsed **1461.455 s old / 1440.662 s Rust** is observational only,
not a controlled performance acceptance or replacement for v3.

## Actual Windows release and supply-chain evidence

Release run 39172092302 executed the following on a disposable Windows runner:

- Removed preinstalled Python distributions and `py`/`pyw` launchers; verified
  `python`, `python3` and `py` unavailable before build (not PATH hiding alone).
- Built CLI and self-contained WPF bundles, collected **223 Cargo packages**,
  and generated a supply-chain manifest covering **423 files**.
- Blocked actual app/OCR outbound traffic with Windows firewall rules; public
  real-ROI video passed OCR, legacy report, trim, decode, duration and source-hash
  checks. Model hash/dictionary/inference and read-only bundle smoke passed.
- Ran **27 WPF tests, zero failed/skipped**, and verified the PerMonitorV2 PE
  manifest.
- Expanded the **final UI ZIP**, launched its self-contained GUI executable
  directly, populated controls through UIAutomation, invoked Run, waited for
  summary and re-enabled RunButton, and required included metric 1 plus actual
  OCR-to-trim/report/media/hash checks. Timeout fails; no mock or silent fallback.
  GUI/CLI/OCR outbound blocking applied; no runtime installation occurred.
- Both archives passed prohibited Python/PyInstaller/Python OpenCV/NumPy/Paddle
  runtime-name inspection. Publish-by-tag was skipped: artifacts were uploaded,
  not a tagged release published.

Independent read-only artifact extraction verified **397 shipped license notice
byte hashes**, every one matching both `INVENTORY.json` and the supply-chain
manifest. Cargo inventory distinguishes 152 shipped and 71 build-only packages;
`simd_helpers` build-only metadata warning is not presented as a runtime license.
Native inference/model/FFmpeg notices remain part of bundle assurance.

| Public artifact | Uploaded artifact-wrapper bytes |
|---|---:|
| [CLI 12153600952](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39172092302/artifacts/12153600952) | 192578943 |
| [UI 12153740351](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39172092302/artifacts/12153740351) | 251828238 |

These are GitHub artifact-wrapper sizes, **not** exact inner distributable ZIP or
unpacked sizes. Separate local final3 output measurements (`xtask size` and exact
filesystem byte/file counts recorded by the coordinator) are:

| Local final3 bundle | Actual distributable ZIP bytes | Unpacked bytes | Files |
|---|---:|---:|---:|
| CLI | 193252797 | 379108228 | 424 |
| UI | 252492771 | 444075133 | 430 |

These measurements describe local final3 artifacts, not the remote artifact
wrappers or byte-identical reconstructions of the CI ZIPs. They establish actual
local package size, not a normalized old/new size delta or improvement; matching
old size evidence is not available here.

## All 22 user stories: requirement-to-evidence matrix

Test paths below are under `rust/ocr_engine/tests/` unless stated otherwise.
Execution evidence combines recorded local real-model/media runs, private corpus,
and successful release CI; latest full Rust CI remains pending as stated above.
A listed test is not by itself a claim that latest CI executed it.

| Story | Requirement | Actual test / evidence |
|---|---|---|
| 1 | Offline Python-free CLI/GUI | Successful clean-host final ZIP CLI/public-media and real GUI UIAutomation release gates above. |
| 2 | Python-free dev/test/build/release | Official Cargo/PowerShell/.NET workflows and retirement source/call audit; release executed without Python; latest Rust test CI pending. |
| 3 | CLI defaults/validation/exits | `pubg_highlight_trim_cli`, `application_contract`, `cli_float_contract`, `nonfinite_steps`, `repeated_files`; old frozen flags and last-files-list behavior preserved. |
| 4 | WPF lifecycle/progress/errors/cancel | 27 executed WPF tests, actual process-tree cancel and local GUI operation; final ZIP GUI completion in release. Old wording issue #74 remains separate. |
| 5 | Three languages/inference | `source_profiles`, `application_files`, `events_contract`; audited real ROI languages 8 Hans / 4 Hant / 6 English. |
| 6 | Own/self events and target | `events_exact_python`, `events_identity_python`, `frozen_baseline_events`; final full-corpus 112 events unchanged. |
| 7 | Assist/delayed/noise/identity | Exact event and scanner fixtures, `molotov_contract`, 18 real ROI semantic gate and three missed-event ROI tests. Existing actor-noise issue #73 is not silently fixed or hidden. |
| 8 | Multi-event/match-end/candidates | `scanner` 33 scenarios, `scan_schedules` 1,540 schedules, candidate/full CLI cases and full-corpus CSV parity. |
| 9 | Refine/context/merge | `planning` 45 frozen cases plus 1,500 differential random cases; `highlight_plan_cli`, `cross_source_duplicates`, `dedupe_cli`; #64 upstream behavior retained. |
| 10 | One/auto/explicit bounded workers | Local single-worker replay and final 95-source auto/explicit parity, actual worker-count and creation-tree audits; `application_e2e` ordering/session/cancel. |
| 11 | H.264/AV1 software seeks/fallback | `media` real generated H.264/AV1 seek/trim/concat and original 4× AV1 median smoke; 72/23 full-corpus split, typed media failure tests. |
| 12 | Fault classification | `media_errors`, `media_launch_failure`, `media_output`, `application_e2e`: source unreadable versus system/model/timeout/single-frame faults; provider diagnostics. |
| 13 | Cache safety | `persistent_cache`, CLI corruption/probe/config/model/content identity and candidate order tests; atomic namespace/checksum and cache-hit provider rebinding. |
| 14 | Reports/names/log contract | `application_reports`, `frozen_report_contract` (42 CSV columns), CLI wire/nonfinite tests, full-corpus 33/25/25 exact fields; independent verbose/profile stderr review. |
| 15 | Original safety | Output/input ancestor/alias/hardlink/staging/archive journal guards, `application_e2e` temporary-copy archive/partial failure/cancel; all 95 SHA256 unchanged. |
| 16 | Retire each old implementation/test | [Retirement inventory](python-retirement-audit.md); no tracked project Python or duplicated production planner/parser. |
| 17 | Temporary bridge only in migration | Final direct Rust library calls; temporary `ocr_engine --plan-clips` removed. Independent engine JSONL and canonical `highlight_plan` remain diagnostics with explicit scope. |
| 18 | Fixed language-neutral oracles | 17,304 differential event cases, 7,227 Unicode and 1,542 identity cases; 27 true frozen baseline cases, 72 meaningful literals, image bytes/planning/schedules/ROI references. |
| 19 | Buildable rollbackable nodes | Dependency commits/PR #72 retain Git history; rollback is Git-based, no Python fallback. Successful production release and pending latest full Rust CI distinguished. |
| 20 | Private local-only regression | Local-only source inventory/reports and hashes; this ledger publishes aggregates only; public ROI/synthetic fixtures supply CI coverage. |
| 21 | Model/tools/license/archive checks | `xtask/tests/{retirement_contract,tools_parity,real_roi,build_real_roi,frame_bench,licenses,cli}` plus successful release archive/397-notice evidence. |
| 22 | Time/RSS/startup/size | Audited v3 table, preserved failed zero-growth time gate plus explicit user acceptance; sampled startup and artifact-size limitations stated, not invented. |

## Implementation and testing decisions: final reconciliation

- Windows x64, thin production CLI and deep Rust modules remain; WPF, FFmpeg,
  ORT and PowerShell are retained rather than reimplemented. Direct OCR library
  sessions are owned per bounded worker/language; CPU default, explicit DirectML
  and fallback diagnostics remain. Actual local DirectML two-event and CPU
  fallback runs supplement provider tests, not a guarantee for all GPU hardware.
- ADR-0001 stdout lifecycle/UTF-8 is preserved; diagnostics go only to stderr
  under verbose, not profile. ADR-0002 keeps software per-seek decoding by
  default and explicit hardware failure fallback, including native NVENC retry.
- Frozen Unicode/event/planning/scanner behavior and eight fixed image-byte
  parity fixtures prevent replacement with superficially similar regex/image
  defaults. Public ROI `reference.json` v2 is hash-bound to actual old-engine
  event semantics, not historically incorrect annotations. Real 18-ROI semantic
  gate passed locally; CI execution is independently required below.
- Cache ABI may change safely; no Python cache/backend is supported. Full-content,
  configuration, model identity, corruption/atomicity and deterministic ordering
  are covered. Safety corrections are explicit, not preserved dangerous behavior.
- Tools retain pinned HTTPS/hash/atomic download, dictionary uniqueness,
  full ORT model load **and real detection plus recognition** with
  `xtask verify --check-onnx --engine`, CPU smoke, semantic compare, ROI/frame
  benchmark, manifest/audit/size/licenses. Protobuf/header checks alone do not
  substitute for inference. Obsolete Paddle prefetch and PyInstaller pruning
  have documented retirement reasons.
- CLI is the principal black-box seam: public/synthetic real media, actual reports,
  faults/cancel and WPF process contracts complement fixed fixtures. Private full
  corpus is local-only; no source relocation/upload or original-move test occurs.
  Temporary copies test destructive archive cases. Encoded output bytes need not
  match, while source integrity, semantics, readability and duration must.
- Independent [two-axis review](rust-migration-review.md) records all findings,
  fixes and rejected speculative changes. Ordinary #73/#74 remain tracked, not
  mixed into migration. Upstream #64 is preserved in the final oracle and Rust.

### Latest CI executed-gates addendum — pending at ledger snapshot

Observed run **39186555704 / `53fce44`** is pending in this snapshot. The final
documentation commit may supersede it: consult [PR #72 checks](https://github.com/SawXu/pubg-highlight-trim/pull/72/checks)
and the coordinator's final executed-gates PR/tracker comment for the exact final
head, run URL and completed outcome. That remote addendum can complete this record
without a documentation-only CI-trigger loop. Required results:
format; default all-features tests including real media; planner; optional feature
checks; formal binaries; pinned download/model full-load/inference; ignored
`end_to_end`; CPU smoke; 18-ROI semantic parity; ignored `application_e2e`,
`dedupe_cli`, `nonfinite_summary`, three-ROI `preprocess`, `ppocr_cpu_smoke_test`,
and `retirement_contract bad_https_download`; bundle assurance and shipped-license
coverage. Earlier release success or default-suite ignored counts cannot replace
these executed results. Until the final-head remote addendum records actual
completion, this CI requirement is **pending**, not presumed green.
