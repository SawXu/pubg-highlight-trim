# ADR 0003: Default OCR inference provider is auto

## Status

Accepted — issue #80, following the explicit provider selection in #79.

## Decision

Change the CLI built-in OCR provider from `cpu` to `auto`. CLI > environment > built-in default precedence remains unchanged. The desktop UI inherits this default without UI changes.

`auto` requests DirectML (DirectX 12) on the configured device, default 0, by actually constructing OCR sessions. Do not enumerate DXGI hardware or add a hardware-discovery dependency. A quick DLL loader preflight precedes session creation; unavailable DirectML falls back to CPU, including inference-time failures. `directml-only` remains strict; explicit `cpu` never attempts DirectML.

Provider diagnostics in summary/profile JSON add `provider_mode`, preserving existing field names and types. `provider_requested` remains the attempted backend, so auto reports `directml`, not `auto`. Reasons distinguish missing `directml.dll`, no usable DX12 device, and other initialization errors, retaining sanitized underlying diagnostics. Cache hits report the current mode even when auto and directml share an equivalent backend/device/fallback cache key.

ADR 0001 still applies: provider diagnostic logs require `--verbose`; default lifecycle output does not gain diagnostic lines. Structured report diagnostics remain available without verbose logging.

## Consequences and risks

- Compatible systems use the GPU without additional user configuration. Missing DLLs, unsupported devices, or broken drivers do not prevent a permissive task from completing on CPU.
- Machines without a discrete GPU may use a slower integrated GPU. Startup/session creation can also cost more than CPU-only execution; auto is availability selection, not a performance benchmark.
- Restore v0.3.6 CPU inference behavior with `--ocr-provider cpu` (or `PUBG_OCR_PROVIDER=cpu`), leaving all other options unchanged.
- CPU and auto caches are isolated by requested backend. A cached fallback records the backend used for that scan; a fresh scan is needed to re-evaluate availability after a driver/hardware change.
- Windows assurance executes the real CLI with pinned models and no provider override, requiring CPU fallback on GPU-less CI, alongside explicit CPU, strict failure, cache, and logging checks. Bundle smoke also validates the shipped executable's default selection.
