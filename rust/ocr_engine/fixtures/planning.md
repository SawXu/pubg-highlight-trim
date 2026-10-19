# Production structured clip-planning baseline

`planning.json` contains 45 fixed input/expected-output cases captured from the
real Python pipeline in commit `75916e2`, before removal of its planning rules.
The one-off capture script and its execution notes live outside the repository;
no Python reference planner is kept in the test suite.

Boundary: classified `EventDetection` records plus configuration → clip bounds,
fully merged report records, and early skipped records. OCR text parsing is not
part of this boundary. Rust executes the golden comparison. Rust application
and CLI integration tests cover reporting, output names, split exports,
trim/concat invocation and exit status through the production Rust application.

Coverage includes:

- stable chronological merge and touching windows with the original `1e-6`
  tolerance; empty input, absent timestamps, duration bounds and 0.1 s recovery;
- minimum event time (including null/negative config), skip-method replacement;
- exact language-profile weapon rules, Unicode whitespace and English Python
  `re.IGNORECASE` semantics; canonical event-key action rather than a new action
  field; elimination-only context, disabled/null extension, and larger defaults;
- target split in `self-death`, then `own-kill` order (unknown targets excluded);
- first duration/timestamp, last detector/timings/counters/budgets/reasons,
  stable deduplication of methods/text/weapons, repeated scores/keys,
  three-decimal timestamps and rebuilt singleton metadata;
- all four removed Python planner unit-test behaviors as fixed cases.

An additional Rust JSON transport regression asserts exact finite float parsing
(`serde_json/float_roundtrip`). A one-off seeded 1,500-case differential run
against Git-history Python produced zero differences after enabling this feature;
without it, 164 cases differed by roughly one ULP. The generator/reference code
is not retained in the repository or required by the Rust tests.

## Known historical behavior deliberately preserved

The Python implementation sorts by event timestamp, not clip start. If a later
Molotov event has a longer before window, overlapping groups extend their end
but **do not rewind their start**. `variable-before-legacy-start-does-not-rewind`
records this behavior. It is not corrected during this migration (#63 forbids
mixing ordinary old-defect fixes into behavior-equivalent migration).

Out-of-duration events and zero-length/beyond-duration plans are also not
silently filtered or turned into synthetic match-end windows: the original
planner passes these through. Scanning/validation is responsible for producing
valid production timestamps. Their fixed cases prevent the old simplified Rust
planner's extra filtering from being reintroduced.

## Canonical diagnostic transport

`highlight_plan` accepts one planning JSON object per line and emits one result
per line, without loading models or initializing inference. Invalid input exits
nonzero with diagnostics on stderr; earlier complete responses are preserved.
It accepts the structured request for developer diagnostics in addition to its
historical text-fixture mode. This is a fixed-data planning diagnostic, **not a
model-free video/OCR preview**: it cannot establish detection quality or replace
real-model end-to-end validation.

The temporary Python bridge and `ocr_engine.exe --plan-clips` entry point have
been retired. Production Rust calls the canonical planner directly; protocol
coverage now runs against `highlight_plan`. The earlier unshipped simplified
`detection_config` schema was removed, not retained as a second rule set.
