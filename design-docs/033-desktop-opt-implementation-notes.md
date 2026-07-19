# P033 Implementation Notes

## Scope

P033 implements the confirmed Desktop conversation optimization, shared multi-model configuration, Context occupancy/compression settings, Token usage visualization, and per-request timing for Terminal and Desktop. Release version: `0.3.0`.

## Implemented contracts

- Shared config schema v2: `activeModel`, `models[]`, `maxContextTokens`, and `context.compression`; v1 remains readable and is migrated only on an explicit Desktop save.
- User-file writer: regular-file/size checks, secret-preserving write-only API Key semantics, SHA-256 fingerprint/CAS, same-directory temporary file, and atomic replacement.
- Request observability: content-free `agent_request_metric` rows plus existing provider Usage, global per-user telemetry DB, WAL/busy timeout, Terminal/Desktop entrypoint, stable request ID, status, Context Token and phase durations.
- Timing semantics: `wallDurationMs` uses a monotonic clock; `activeDurationMs = wall - approvalWait`; Terminal and Desktop show a live elapsed value and replace it with the final Runtime value.
- Context compression: built-in `recent-window` and `extractive-summary`, configurable threshold/recent count, public Rust reducer SPI, and content-free `ContextAccessSnapshot`.
- Desktop bridge: settings load/save and Runtime rebuild, Usage query, history load, nested project tree, recent projects, and process-local permission switching.
- Desktop UI: conversation-first four-region layout, `+`/`/` input actions, Context ring/tooltip, model CRUD, Usage calendar/chart/history, light/dark and zh-CN/en preferences, responsive fallback, while preserving advanced workspaces.

## Security and compatibility

- API keys are never returned by settings APIs or written to telemetry; blank edits retain the existing secret by model name.
- Desktop writes only the resolved user configuration location. Project and environment layers remain read-only and may still override the effective Runtime.
- Observability failures are fail-open for successful inference, and the response reports whether telemetry was recorded.
- No dynamic compression binary/script loading, cloud synchronization, billing claims, multi-Runtime background concurrency, session deletion, or irreversible retention policy was added.

## Verification evidence

Pending the unified final gate. Results will be recorded after implementation, formatting, unit assertions, end-to-end tests, static checks, and three review passes complete.
