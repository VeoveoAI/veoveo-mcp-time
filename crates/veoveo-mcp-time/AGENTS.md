# Time MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is the
[hosted MCP server contract](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/a1d9d653ae3d723821cdeeab92f4a67acdca39ce/crates/veoveo-hub-sdk/DESIGN.md) in
`veoveo-hub-sdk`, and registration follows its
[server registration protocol](https://github.com/VeoveoAI/veoveo-hub-sdk/blob/a1d9d653ae3d723821cdeeab92f4a67acdca39ce/crates/veoveo-hub-sdk/src/registration/DESIGN.md).

## Purpose

One temporal authority for civil time, military date time groups, GNSS time,
epochs, operational calendars, clock quality, and events.
Every resolved instant carries the authority releases and uncertainty used to
interpret it, so other servers consume time without reconstructing timezone or
leap second assumptions.

## Invariants

- The server is hosted through `veoveo_hub_sdk::hosting`: `TimeMcp` implements
  `DomainServer` and `TimeSubscriptions` implements `ResourceSubscriptions`. Tenant
  administration is MCP tools in `src/mcp/administration.rs`, joined to the domain
  tools by its ready typed `Routes` table. `/readyz` at the listener root reports ready while the
  clock is observed. Do not add a `ServerHandler`, host check or token verification here.
- Typed `Routes` construction binds each tool request and resource reader to its
  output, permission and cache policy. Registration, discovery, annotation hints and
  dispatch derive from that table; do not author a second requirements registry.
- Time registers with the Hub before serving and takes its access-token trust and
  database login from the registration. Do not add static trust, JWKS or database
  credentials to configuration. The registration's HTTP client also refreshes the
  forwarded-token keys from the issuer's RFC 8414 metadata, which Time reads through
  the Hub's native HTTPS application origin, with verified hostname and provisioned
  workload CA trust. Native `resume_or_register` retains the same immutable deployment
  and key across process restarts; `maintain` does not delete on process stop.
  Explicit retirement deletes registration, and the Hub keeps its existing orphan
  grace. Do not add renewal loops or a global-bootstrap fallback when resume fails.
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
- Durable state lives in the PostgreSQL schema `time` and the authority release
  volume under `/var/lib/veoveo/time`. Time applies `migrations/` at startup as
  `server_time`. Every statement is a `.sql` file under `src/persistence/queries/`
  run through sqlx's checked macros; regenerate `.sqlx` after a query or migration
  change. Row triggers notify the `time` channel; subscribers hear only their own
  tenant through `TenantSubscriptions`. Do not notify subscribers from handlers.
  Tenant engine caches are derived and rebuilt from the active release pair.
- Recorded timestamps carry microsecond precision. Pass a timestamp a writer records
  through `admission::stored_timestamp`, or `stored_deadline` for a due instant,
  before serializing or binding it; never relax the body-column comparisons instead.
- Events become due only through the per-process due loop in `src/due.rs`, which runs
  one conditional `UPDATE` per batch of the earliest events until a batch comes back
  short. Keep the batch bound and `SKIP LOCKED`. Do not add in-process timers per event.
- Every collection and administration listing is a keyset page with separate
  first-page and next-page statements. Do not reintroduce a `$cursor IS NULL OR …`
  predicate, which loses the index seek in a cached generic plan. Completion matches
  ID prefixes as `COLLATE "C"` key ranges, never as substrings or `LIKE` patterns.
- Authority activation is atomic: optimistic versions plus a full preflight
  load of the prospective TZDB and leap second pair; one active release per
  family per tenant. Acquisition downloads run under fixed host, media,
  digest, size, and time policy with archive traversal rejected.
- Every listing, including bounded `list_active_authorities`, uses the native
  `Collection`/`Pager` and canonical `Page` contract. Active entries retain
  `{pointerVersion, release}` and `selection.write_guard()`; pointer and release
  versions advance independently. Preserve joined tenant/parent/body admission.
- Packaged bootstrap references bind the authority family and source-file SHA-256.
  Their immutable resource IDs must change with the bytes, never with a process
  restart or path. `server/bootstrap.rs` owns construction and qualification.
- `expand_schedule` and `validate_timeline` run only through the SDK's MCP Tasks
  adapter over core's `veoveo-task`; they require the per-request Tasks extension
  capability and reject missing support with standard `MissingRequiredClientCapability`,
  while Task creation remains server-directed. Creating a Task only queues it; `server::tasks::work`
  consumes the runtime's claim stream and runs what this instance claims. Recovery
  of a dead instance's Tasks is the kernel's; do not add leases, heartbeats or a
  recovery observer. Hosted calculations use Tokio's blocking pool. Retain the
  original job outside the cancellable waiter and await it before the worker exits.
  Preserve cooperative stop checks; sorting and serialization between checks do not
  supply a hard abort or drain-time guarantee. Public engine methods stay synchronous
  for library callers.
- Civil fold and gap resolution defaults to `reject`; military zone `J` is
  rejected by the DTG parser. A positive leap second keeps its `:60`
  representation.

## Build And Test

- `cargo check -p veoveo-mcp-time`
- `tests/acceptance.rs` runs through a live local installation from this repository's
  environment (see the README's Acceptance section): discovery and tool calls as the
  member and admin bearers selected by `VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV` and
  `VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV`, refused unauthenticated and undeclared requests,
  administration refused to the member and admitted for the admin, a schedule Task whose
  completion and result arrive as a pushed status notification, a Working schedule Task
  whose cancellation from another member connection is pushed and retained after a
  reconnect, an event whose Due state
  follows a pushed update of its resource, and current state after a reconnect. A test
  that claims a push polls nothing before the change and requires it to follow the state
  the subscription opened with. The tests are ignored by default; run them from a fresh
  nextest archive extraction. Input-only tests are not ignored and inject their lookup
  rather than mutating process environment. Do not copy caller private keys into a
  native factory; its access preflight receives only the member reference. Exclude
  `restart_persistence_` and `a_running_schedule_task_recovers_after_restart` from
  the ordinary ignored suite; run the persistence prepare
  and verify phases around trusted operator restarts, as the README describes.
  These phases prove retained terminal event/Task state with fresh subscriptions,
  not in-flight Task recovery or the runtime restart itself. Run the explicit
  in-flight case alone with native streamed stdin/stdout and trusted operator
  restart evidence. It requires the same Task Working after restart, opening
  Working and then pushed Completed with the expected typed schedule. Early
  completion is inconclusive evidence and must fail.
- `cargo test -p veoveo-mcp-time --lib server::tasks::tests` runs Time's consumer, the
  SDK's `TaskWorker`, on two process instances on separate pools. The SDK qualifies the
  worker's settlement seams; these tests qualify Time's checkpoints. A queued Task is
  claimed by a replica that did not create it. `server::tasks::tests::recovery` stalls a
  calculation behind a lock on the authority pointers. When the replica's instance lock
  connection ends and the kernel reaper removes its instance meanwhile, the Task returns
  to the queue, the replica stops the worker that ran under the reaped instance, claims
  the Task again under a new instance and completes it with the result the engine
  expands directly. When the first replica's process stops instead, its calculation
  publishes nothing, invents no cancellation and is finished by the second replica after
  recovery. A cancellation requested on another replica settles the Task as cancelled at
  the next checkpoint. Keep each stall deterministic. The ordinary live suite
  qualifies installed cancellation only when run against the deployed installation;
  it supplies no restart proof. The explicit running-Task restart case qualifies its
  narrow recovery claim only with independently verified native lifecycle evidence
  that the old runtime stopped and its replacement became ready. Terminal event/Task
  persistence is checked separately by the prepare and verify phases.
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
- `cargo test -p veoveo-mcp-time --lib registry::tests::permissions` checks allowed
  and denied current route permissions through the hosted router.
- `cargo test -p veoveo-mcp-time --no-default-features --features contract` checks
  public contract validation and compile-fail examples. Qualify dependency isolation
  with a separately resolved consumer workspace; a workspace-wide build may enable
  dependencies through other packages.
- `cargo clippy -p veoveo-mcp-time --no-default-features --features runtime --all-targets -- -D warnings`
  checks runtime composition independently from the MCP feature.
- Time owns its private `src/persistence/` queries, driver records and mutation
  validation, and its schema in `migrations/`. Activation takes the tenant's
  transaction advisory lock, which covers absent pointers, then locks the pointers and
  preflight releases with `FOR UPDATE`. Preserve full snapshot comparison and the typed
  `ActivationConflict` outcomes. Runtime library tests use `test_database::TimeDb`:
  `veoveo-fixtures`' PostgreSQL cluster, Time's migrations applied as
  `server_time` and two pools logged in as registration logins. Run them with the
  PostgreSQL 18 programs on `PATH`.
- `docker build --target check .` runs the gate, and the `runtime` stage builds the
  deployable image. No GPU requirement.
- `src/catalog/tests.rs` qualifies SQL tenant and owner predicates, latest epoch
  selection, event transition isolation, page boundaries, requested epoch batches,
  and bounded distinct prefix completion with `LIKE` metacharacters taken literally.
  `src/catalog/tests/admin_pages.rs` walks the source and acquisition pages.
- `cargo test -p veoveo-mcp-time --lib persistence::plans` explains the next-page, ID
  completion and due statements with sequential scans disabled and requires each to
  seek its index. The `explain_*.sql` files in `src/tests/queries/` repeat production
  statements verbatim, and a unit test fails when one drifts. Add a statement there when
  a new query's cost grows with a tenant's rows.
- `cargo test -p veoveo-mcp-time --lib persistence::conformance` runs core's database
  conformance check over every statement under `src/persistence` and the `time` schema.
  A statement binds its tenant, bounds its rows and reaches them through an index in a
  generic plan, or it carries a `-- veoveo:` marker that states why it cannot. A table
  needs a primary key, an index whose leading columns are each foreign key's, and a
  `Retention:` comment, added in a new migration. Regenerate `.sqlx` before running it,
  because the check reads the statements from that metadata.
- `src/catalog/tests/metadata.rs` qualifies retained body/key agreement, the schema's
  refusal of corrupt columns, redacted diagnostics and immutable acquisition metadata.
  Corruption goes through `src/tests/queries/`. Preserve the lifecycle-column rules in
  the design's Retained Catalog Metadata section; stale release-state bodies can
  represent valid retired records.
- `src/contract/clock_policy.rs` owns the checked policy builder and JSON bounds.
  Positive `TimeVersion` guards and optional-row `TimeWriteGuard` guards stay typed
  through writes; numeric zero means absence only for clock and active-pointer writes.
  All public metadata and immutable calendar/epoch versions use `TimeVersion`.
  Source creation takes `NewTimeSource` with the zero-only `SourceCreationVersion`.
  Check every increment; avoid converting a metadata version back through a raw integer.
- Catalog bodies decode through the current public metadata types. Lifecycle columns
  supply current state and version; zero and overflowing body versions are rejected.
  Historical formats have no adapter. The foundations installation drains and resets.
- Calendar, epoch and release reads are shared within the tenant, and events are
  readable only by their owner. Keep packaged bootstrap references separate from
  tenant-acquired releases, and keep epoch version URIs distinct in enumeration.
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
- `src/catalog/tests/numeric.rs` qualifies the schema's clock scalar ranges, optimistic
  clock writes and exhausted versions without row mutation. Authority qualification
  verifies that retirement exhaustion rolls back both the candidate and pointer.
- `src/persistence/active_tests.rs` qualifies pointer/release selection, the schema's
  relationship constraints and relationship changes inside activation through fixture
  triggers. Keep relationship predicates in SQL and reject a visible dangling pointer.
  Bootstrap selection during reload requires an absent pointer.
- `src/registry/tests.rs` uses `/usr/share/zoneinfo/UTC` and owned temporary files.
  Engine requests validate the active pair and provenance before cache reuse. Each
  returned engine owns its epoch map. A tenant's authority notification and every
  reconciliation evict cached contexts; delivery never substitutes for a read.
  Authority loading and preflight each have a 30-second deadline.
- `src/registry/tests/activation.rs` qualifies cross-family conflicts, absent and
  present pointers, the locks that hold off concurrent activations and repairs, and
  stale preflight metadata. Production activation goes through the registry and
  consumes its observed draft.
- `src/registry/tests/notifications.rs` qualifies tenant-scoped resource updates and
  authority eviction from real triggers and `LISTEN`; `src/registry/tests/due.rs`
  qualifies that overdue events become due at process start without a read, that
  replicas change each event once, and that a backlog larger than a batch drains in
  short statements.
- The optional ntpd-rs observation socket is a deployment concern; unit tests
  use bounded fake observations.



### Installed Future Events

`an_event_reaches_due_with_a_notification` and
`a_reconnected_caller_reconciles_current_state` in `tests/acceptance.rs` qualify future
events through the Hub. The caller creates an event ten seconds ahead and subscribes
to its own resource. The subscription's opening update reads Scheduled, and the event
must read Due after a later update. When the subscription opens on a Due event, the
host was slower than the lead, so the test repeats with leads of 30 and then 90
seconds. A subscriber that disconnects while its event is Scheduled reconnects after
another session cancelled the event. Its new subscription opens with an update that
leads to a read of exactly the cancelled event. These tests wait on observed states and
updates, each bounded by a timeout, and never on a fixed sleep.
