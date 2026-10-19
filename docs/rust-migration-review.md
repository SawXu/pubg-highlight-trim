# #63 integration review disposition

Fixed point: `a070076526f918b0ef7f1848994d69363d5c1291`. The first independent
Standards and Spec reviews inspected `32d2990`; corrections were implemented by
one review implementer (`3d9c949`, `8a53b45`) and rechecked against the integrated
`c4e4a46` state. This review does not replace final private-corpus/performance,
clean-host CI or actual archive acceptance.

## Standards

All three documented-rule findings were corrected:

1. New provider telemetry uses a synthetic 1-based `source_index`, including
   cache-hit rebinding, rather than copying source paths into additional fields.
   Fallback diagnostics retain actionable causes with model/operator paths
   redacted. Existing mandatory report paths/columns remain compatible as the
   user explicitly required.
2. New ROI-builder metadata uses synthetic source labels, source-content hashes
   and version-2 OCR/event fingerprints; decode errors do not repeat source or
   player names. Original-source preference, selection and crop checks remain.
3. Media diagnostics are forwarded to stderr only under `--verbose`.
   `--profile` does not enable them. The actual CLI marker-wrapper acceptance
   exercises default, profile and verbose modes plus failed NVENC/software retry.

The independent follow-up read the corrected code and found those three items
resolved. No material Fowler smell was reported; formatting remains tool-enforced.
Legal upstream notice text and necessary source/model fingerprints are preserved
as required; they are not private operator metadata.

## Spec

The eight initial observations were individually checked, rather than treating
review prose as proof or changing correct behavior speculatively:

| Finding | Disposition and executable evidence |
|---|---|
| Systemic probe errors classified unreadable | **Fixed.** Only typed `VideoUnreadable` is a source skip; other command failures/timeouts are fatal and actionable. CLI bad-probe/timeout and good+bad-source tests exercise the distinction. |
| Candidate CSV order changes cache identity | **Fixed.** Candidate times are sorted before key construction; real CLI cache reuse verifies reordered hints. |
| Native NVENC-only failure cannot recover on CPU | **Fixed.** Software retry is restored for native encoder failures; cancellation never retries. Common loader failures remain fatal if the software attempt also fails. Native mock plus CLI diagnostics tests execute both outcomes. |
| A source hard error discards already-generated clips | **Not demonstrated.** The failing scan stage precedes media generation and output preparation. Systemic failure deliberately preserves existing output and originals. Ordinary unreadable sources continue through the typed per-source channel. This is not loss of completed clips. |
| Molotov Unicode rule differs from Python | **Not demonstrated.** The cache compiler already expands dotted/dotless I; the 37-case public helper contract and frozen planning cases match Python. Python `re.IGNORECASE` is not full string casefold (`ß` does not match `ss`), so implementing that proposed expansion would introduce a regression. |
| Effective inference threads should be omitted from cache identity | **Rejected as unsafe generalization.** Actual execution configuration remains part of the identity. With the default one inference thread, jobs 1→2 still hits the cache and rebinds provider ordinals. A real thread-configuration change may safely cause a miss. |
| Error category depends on Display prefix | **Fixed.** The factory returns `MediaError`; the scanner backend retains an explicit failure-kind tag. Display wording is no longer parsed for classification. |
| A general nonfinite JSON writer is required | **No reachable additional case found.** Old clamp/validation rules remove negative infinity/NaN from report metadata; the two reachable positive-infinity summary fields preserve Python's explicit numeric extension. Actual CLI wire tests cover them; finite runs retain ordinary JSON field types. No speculative future writer framework was added. |

The subsequent main-line advance `50c1177` / #65 / #64 was ported independently,
not discarded with Python: conservative cross-recording verification, more
available context, duplicate-skip records and source safety have full batch tests.
Its intended differences are compared against that upstream Python oracle in the
final quiet corpus run, rather than mislabeled migration regressions.

Existing ordinary defects #73 (preceding HUD noise in actor text) and #74 (WPF
unreadable-only completion wording) remain separately tracked, as required by
#63's scope. They are not masked by weakening migration expectations.
