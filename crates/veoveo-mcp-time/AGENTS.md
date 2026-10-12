# Time MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/contract/DESIGN.md),
and registration follows
[`mcp/ext-registration/DESIGN.md`](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/ext-registration/DESIGN.md).

## Purpose

One temporal authority for civil time, military date time groups, GNSS time,
epochs, operational calendars, clock quality, and events.
Every resolved instant carries the authority releases and uncertainty used to
interpret it, so other servers consume time without reconstructing timezone or
leap second assumptions.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `TimeMcp` implements
  `DomainServer` and `TimeSubscriptions` implements `ResourceSubscriptions`. Tenant
  administration is MCP tools in `src/mcp/administration.rs`, joined to the domain
  tools by `TimeMcp::tools()`. `/time/readyz` reports ready while the clock is
  observed. Do not add a `ServerHandler`, host check or token verification here.
- `src/mcp/requirements.rs` is the only source of permission requirements. A new tool,
  fixed resource, template or prompt gets its row there, and its handler checks
  through `require` or `authorize`. The table builds the registration and Discover
  declaration, so the Gateway and Time evaluate the same requirements.
- Time registers with the Gateway before serving and takes its access-token trust and
  database user from the registration. Do not add static trust, JWKS or database
  credentials to configuration.
- Owns the `time://` URI scheme. Identity: slug `time`, MCP `/time/mcp`,
  port 8800.
- The canonical instant is `TimeInstant`: integral TAI seconds plus nanosecond,
  uncertainty, and the TZDB and leap second release ids. Never emit an instant
  without its authority binding. Deterministic results also emit both exact
  release URIs, version labels, canonical SHA-256 digests, and explicit
  bootstrap or acquisition provenance. Intervals are half open `[start, end)`.
- Deterministic resolution and conversion do not contain a live clock
  observation. `time://clock/current` carries the effective clock policy and
  measured quality, including holdover evidence.
- Durable state lives in its SurrealDB owner tables (`time_*`) and the
  authority release volume under `/var/lib/veoveo/time`. The server never
  applies migrations. Tenant engine caches are derived and rebuilt from the
  active release pair.
- Authority activation is atomic: optimistic versions plus a full preflight
  load of the prospective TZDB and leap second pair; one active release per
  family per tenant. Acquisition downloads run under fixed host, media,
  digest, size, and time policy with archive traversal rejected.
- `list_active_authorities` returns `AdminPage<ActiveAuthoritySelection>` with
  closed `{pointerVersion, release}` entries. Use `selection.write_guard()` for
  activation; the persisted pointer version differs from `release.recordVersion`.
  Preserve the joined tenant/parent/body admission and reject non-active releases.
  An absent family uses `TimeWriteGuard::Absent`; packaged effective references
  never supply a persisted pointer version.
- Packaged bootstrap references bind the authority family and source-file SHA-256.
  Their immutable resource IDs must change with the bytes, never with a process
  restart or path. `server/bootstrap.rs` owns construction and qualification.
- `expand_schedule` and `validate_timeline` run only through the final Task
  API extension on `veoveo-task-runtime`; a direct call returns an instruction
  to use the task form. Hosted calculations use Tokio's blocking pool. Retain the
  original job outside the cancellable waiter and await it on normal worker exit,
  including heartbeat failure. Preserve cooperative stop checks; sorting and
  serialization between checks do not supply a hard abort or drain-time guarantee.
  Public engine methods stay synchronous for library callers.
- Civil fold and gap resolution defaults to `reject`; military zone `J` is
  rejected by the DTG parser. A positive leap second keeps its `:60`
  representation.

## Build And Test

- `cargo check -p veoveo-mcp-time`
- `tests/acceptance.rs` runs through a live local installation from this repository's
  environment (see the README's Acceptance section): discovery and tool calls as the
  member and admin callers, refused unauthenticated and undeclared requests,
  administration refused to the member and admitted for the admin, a schedule Task whose
  status notification arrives through its subscription before it completes, an event that
  reaches Due
  with its resource notification, and current state after a reconnect. The tests are
  ignored by default; run them from a fresh nextest archive extraction.
- `cargo test -p veoveo-mcp-time --lib server::tasks::tests` qualifies durable
  cancellation at calculation checkpoints and final settlement using distinct
  workers on the existing isolated Store fixture. Keep the cancellation/completion
  CAS interleave deterministic; do not manufacture a slow installed schedule.
  Preserve completion that committed first, genuine calculation failures and the
  executing worker's current lease. Local shutdown without durable cancel intent
  stops success publication before shared transition dispatch and leaves the request
  for Resume recovery. Keep token checks at both selected-snapshot dispatch seams;
  a transition already entered may settle despite a later local stop. Installed cancellation and unfinished restart
  still require their own observable qualification.
- `cargo test -p veoveo-mcp-time --lib server::tasks::calculation::tests`
  checks async responsiveness during a retained blocking job and joins the original
  job after interruption. `cargo test -p veoveo-mcp-time --lib cooperative_stop`
  checks engine stop handling separately from calculation failures.
- `cargo test -p veoveo-mcp-time --test metadata_versions active_selection_exposes_pointer_guard_and_rejects_unadmitted_wire`
  checks the closed active-selection response and schema. The focused hosted
  `registry::tests::active_authorities::operator_reads_pointer_guard_for_next_activation`
  library control reads the real pointer guard for two successive activations;
  preserve the existing native active-pointer corruption checks.
- `cargo test -p veoveo-mcp-time`
- `cargo test -p veoveo-mcp-time --lib requirements` checks the requirement table
  against the listed tools, advertised resources and tool annotations, and
  `--lib registry::tests::permissions` checks each handler's allowed and denied
  permissions through the hosted router.
- `cargo test -p veoveo-mcp-time --no-default-features --features contract` checks
  public contract validation and compile-fail examples. Qualify dependency isolation
  with a separately resolved consumer workspace; a workspace-wide build may enable
  dependencies through other packages.
- `cargo clippy -p veoveo-mcp-time --no-default-features --features runtime --all-targets -- -D warnings`
  checks runtime composition independently from the MCP feature.
- Time owns its private `src/persistence/` queries, driver records and mutation
  validation. Its `src/schema/` declaration owns the selected Time migration lane.
  Activation uses exact-ID locked reads
  of both pointers, including absence, and their preflight releases. Preserve full
  snapshot comparison and qualify cross-family contention on RocksDB.
  Runtime library tests use the shared
  isolated SurrealDB fixture with the Time lane selected; schema changes require
  fresh lane admission and persistence checks.
- `docker build --target check .` runs the gate, and the `runtime` stage builds the
  deployable image. No GPU requirement.
- `src/catalog/tests.rs` qualifies SQL tenant and owner predicates, latest epoch
  selection, event transition isolation, page boundaries, requested epoch batches,
  and bounded distinct completion against the
  pinned disposable SurrealDB fixture.
- `src/catalog/tests/metadata.rs` qualifies retained body/key agreement, indexed
  ordering fields, redacted diagnostics and immutable acquisition metadata. Preserve
  the lifecycle-column rules and upgrade requirements in the design's Retained Catalog
  Metadata section; stale release-state bodies can represent valid retired records.
- `src/contract/clock_policy.rs` owns the checked policy builder and JSON bounds.
  Positive `TimeVersion` guards and optional-row `TimeWriteGuard` guards stay typed
  through writes; numeric zero means absence only for clock and active-pointer writes.
  All public metadata and immutable calendar/epoch versions use `TimeVersion`.
  Source creation takes `NewTimeSource` with the zero-only `SourceCreationVersion`.
  Check every increment; avoid converting a metadata version back through a raw integer.
- Catalog bodies decode through the current public metadata types. Lifecycle columns
  supply current state and version; zero and overflowing body versions are rejected.
  Historical formats have no adapter. The foundations installation drains and resets.
- `catalog/knowledge.rs` binds observations to SQL-admitted records and stored creation
  provenance. Calendar/epoch/release reads are tenant-shared; events are owner-only.
  Keep packaged bootstrap references separate from tenant-acquired releases, and keep
  epoch version URIs distinct in enumeration. Source access and content both affect revisions.
- `contract/instant.rs` owns subsecond admission and checked total-coordinate
  construction. Keep `SubsecondNanoseconds` through requests, cursors and persistence
  drafts; convert at protocol-library and database-driver adapters. Durations have
  separate ranges. `tests/instant_contract.rs` checks wire/schema bounds, defaults,
  negative fractions and both signed-seconds endpoints. Native metadata checks reject
  matching invalid body/column fractions without rewriting stored rows.
- `contract/authority.rs` owns checked release references, family pairs and instant
  bindings. Derive repeated identity from the typed release URI and derive bindings
  from the effective pair. Keep their fields and the runtime `AuthorityContext`
  private; callers consume accessors. Pair construction checks dataset roles and
  distinct release IDs. `tests/authority_contract.rs` qualifies wire admission and
  schemas, while native metadata tests qualify stricter retained-binding admission.
- `contract/window.rs` owns checked nonempty half-open intervals and intersection.
  Keep bounds private and check the active engine authority at runtime. Algebra and
  schedule clipping preserve selected endpoint uncertainty, taking the maximum at
  tied coordinates without moving the bound. `tests/window_contract.rs` checks wire
  admission; `engine/windows.rs` and engine schedule tests qualify runtime behavior.
- `contract/resolution.rs` binds each resolved instant to matching effective release
  metadata, with read-only accessors and the existing flat projection fields. The
  engine calculates the projections and rejects foreign epoch authorities. Keep
  epoch IDs typed and carry their uncertainty through checked addition. Authority
  contract tests and native engine cases qualify decoding and relative resolution.
- Digests are the foundational `Sha256Digest` (`sha256:` and 64 lowercase hexadecimal
  digits) in requests, persistence drafts and provenance. Keep them typed until the
  driver record. `tests/digest_contract.rs` and `src/registry/tests/digest.rs` qualify
  request admission, idempotency and malformed matching body/column values.
- `src/catalog/tests/numeric.rs` qualifies clock scalar rejection, optimistic clock
  writes and exhausted versions without row mutation. Authority qualification verifies
  that retirement exhaustion rolls back both the candidate and pointer.
- `src/persistence/active_tests.rs` qualifies joined pointer/release selection,
  inconsistent retained records and relationship changes inside activation. Keep
  relationship predicates in SQL and reject a visible dangling pointer. Bootstrap
  selection during reload requires an absent pointer.
- `src/registry/tests.rs` uses Linux `/usr/share/zoneinfo/UTC`, owned temporary files
  and the shared isolated database fixture. Engine requests validate the active pair
  and provenance before cache reuse. Each returned engine owns its epoch map. Cache
  eviction follows every Store invalidation; delivery never substitutes for a read.
  Authority loading and preflight each have a 30-second deadline.
- `src/registry/tests/activation.rs` qualifies cross-family conflicts on disposable
  RocksDB, absent and present pointers, concurrent release repairs and stale preflight
  metadata. Production activation goes through the registry and consumes its observed
  draft. Both exact pointer IDs and all preflight release IDs participate in commit
  conflict checks. Keep the fresh-store and drain requirements in DESIGN.
- The optional ntpd-rs observation socket is a deployment concern; unit tests
  use bounded fake observations.

## Contract Compliance

<!-- veoveo:contract-compliance:start -->
Contract: https://veoveo.ai/contracts/hosted-mcp

- C01: met
- C02: met
- C03: met
- C04: met — stable discovery, SQL-scoped pages of 100 records, exact reads, and bounded completions; native tests cover tenant/owner isolation and cursor ordering
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: met
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met — static discovery advertises no list-change capability; Store observations invalidate resource contents
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — typed docs and five domain collections are implemented; source-policy, provenance, URI and paging checks pass natively; installed K01–K08 qualification and event change/restart probes remain open
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->

### Installed Future Events

`an_event_reaches_due_with_a_notification` and
`a_reconnected_caller_reconciles_current_state` in `tests/acceptance.rs` qualify future
events through the Gateway: the caller subscribes to `time://events`, creates an event
five seconds ahead and reads it as Due after the resource notification, and a caller
that disconnects reads the current state after reconnecting rather than a replay.
