# ADR 0001: CLI logging contract

## Status

Accepted

## Decision

The CLI reserves stdout for core lifecycle output and machine-readable records:

- `PROGRESS ...` reports scan progress.
- `INCLUDE` and `SKIP` lines report per-source decisions.
- `SUMMARY ...` reports the final JSON summary.

Default runs suppress OCR, ffmpeg, worker, and other third-party diagnostics. Errors remain actionable summaries and preserve the existing exit codes. The `--verbose` option enables startup settings and third-party diagnostics, including diagnostics from parallel OCR workers.

The `--profile` option is independent from `--verbose`: it adds only `PROFILE ...` performance measurements and does not enable startup settings or third-party diagnostics. This keeps profiling output useful without changing the normal diagnostic noise level.

## Consequences

Consumers such as the desktop UI can parse stdout without filtering library debug output. Troubleshooting uses `--verbose`, while performance investigations use `--profile` and receive stable timing lines. Tests in `tests/test_cli_logging.py` protect the stream, verbosity, and exit-code contract.
