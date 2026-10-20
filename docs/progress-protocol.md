# PROGRESS JSON protocol (#88)

Stdout reserves `PROGRESS ` followed by one single-line JSON object for phase
progress. Stderr is diagnostic only and must never update progress or summary.

## Fields

| Field | Type | Meaning |
|-------|------|---------|
| `phase` | string | `scan`, `dedupe`, `trim`, `merge`; ignore unknown future phases safely. |
| `event` | string, optional | `start`, `done`, or `skipped`; absent on intermediate updates and legacy scan records. |
| `current` | integer | Completed work units, including `0` on start/skipped. |
| `total` | integer | Work actually scheduled in this phase. |
| `workers` | integer | Selected phase concurrency: OCR scan jobs; effective Trim workers (`1` or `2`); serial dedupe/merge `1`; no work/skipped `0`. Not a live thread census. |
| `reason` | string, optional | On skipped phases: `dry-run`, `disabled`, or `no-clips`. |
| `planned` | integer, optional | On skipped phases; dry-run trim retains the number of planned clips, without counting them as completed. |

All emitted records retain `phase`, `current`, `total`, and `workers`. Consumers
must continue accepting older scan records without `event` and ignore unknown
fields. The scan sequence remains `0/N` through `N/N` without extra lifecycle
records for compatibility. New non-scan lifecycle records explicitly terminate
a phase instead of leaving a spinner running.

## Units and lifecycle

- **Scan:** source videos completed; `workers` is the selected OCR job count.
- **Dedupe:** keyed event candidates checked, in candidate order (not pair count).
  `start 0/N`, intermediate `k/N`, then `done N/N`. Zero candidates emit
  `start 0/0 workers=0`, then `done 0/0 workers=0`.
- **Trim:** clips successfully encoded, validated, and published. Normal runs
  emit `start 0/N`, intermediate `k/N`, then `done N/N`. Zero clips emit start
  and done `0/0 workers=0`. Dry-run emits only `skipped 0/0 workers=0`,
  `reason:dry-run`, `planned:N`; no encoding or publication is implied.
  `--trim-jobs auto|1|2` is independent of OCR `--jobs`; auto conservatively
  selects one worker. Normal Trim uses `min(resolved request, clip count)`.
  A bounded window holds at most that many running or waiting-to-publish tasks.
  Completion advances only after strict validation and ordered publication,
  not when an out-of-order encoder finishes. Stable clip/Merge ordering is
  unchanged; a slow early task can temporarily leave progress unchanged.
- **Merge:** published merge targets, independently counted (including split
  targets). Nonempty work emits start, intermediate updates, then done.
  Otherwise emit `skipped 0/0 workers=0`: `disabled` when merge is disabled,
  `dry-run` when enabled but dry-run, or `no-clips` when enabled without clips.

## Example

```text
PROGRESS {"phase":"scan","current":0,"total":1,"workers":1}
PROGRESS {"phase":"scan","current":1,"total":1,"workers":1}
PROGRESS {"phase":"dedupe","event":"start","current":0,"total":1,"workers":1}
PROGRESS {"phase":"dedupe","current":1,"total":1,"workers":1}
PROGRESS {"phase":"dedupe","event":"done","current":1,"total":1,"workers":1}
PROGRESS {"phase":"trim","event":"start","current":0,"total":1,"workers":1}
PROGRESS {"phase":"trim","current":1,"total":1,"workers":1}
PROGRESS {"phase":"trim","event":"done","current":1,"total":1,"workers":1}
PROGRESS {"phase":"merge","event":"skipped","current":0,"total":0,"workers":0,"reason":"disabled","planned":0}
```

For example, three planned clips with `--trim-jobs 2` start with
`PROGRESS {"phase":"trim","event":"start","current":0,"total":3,"workers":2}`
and finish with `current:3,total:3,workers:2`. One planned clip uses `workers:1`.
The worker count is a capacity, not a promise that both encoders are busy.
See [profile metadata](profile-schema.md) for requested versus effective counts.

## Invariants and GUI compatibility

- `0 <= current <= total`; failed or cancelled work must not emit phase done.
  A completed earlier phase may remain done if a later phase fails.
- A skipped phase is terminal, not an active encoder or merge operation.
- GUI displays phase, worker count (including `0` and `1`), running/completed/
  skipped state, and a checkmark only for completed stages.
- Legacy `[N/M] INCLUDE/SKIP` lines still update decision counters and CSV/
  summary semantics remain unchanged (dry-run INCLUDE is a planned decision).
  They cannot overwrite activity once structured phase progress is active.
- Unknown future phases must not replace visible known activity; subsequent
  legacy lines also must not overwrite the structured activity.
- Reruns reset summary, counters, structured-phase latch, and visible metrics,
  including after cancellation or failure.
