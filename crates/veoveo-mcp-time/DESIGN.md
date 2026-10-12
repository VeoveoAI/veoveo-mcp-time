# Time MCP Design

This document is the design and operational contract for the `veoveo-mcp-time`
crate.

Time gives agents one temporal authority for civil time, military date-time groups,
GNSS time, epochs, operational calendars, clock quality, and events. Every resolved instant carries the authority releases and
uncertainty used to interpret it. Map and Optimization can therefore consume time
without reconstructing timezone or leap-second assumptions.

## Service Identity

```text
crate        veoveo-mcp-time
binary       veoveo-mcp-time
repository   veoveo-mcp-time
slug         time
URI scheme   time
permissions  time.*
MCP          /time/mcp
admin REST   /time/admin
liveness     /time/livez
readiness    /time/readyz (ready while the clock is observed)
port         8800
```

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Version `2026-07-28`, JSON-RPC 2.0 over Streamable HTTP, under the [hosted MCP contract](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/contract/DESIGN.md) `https://veoveo.ai/contracts/hosted-mcp`. The server exposes tools, resources and templates, prompts, completions, subscriptions, notifications, and typed structured content. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Temporal expressions, authority bindings, calendars, epochs, windows, clock evidence, tasks, and results. |
| [Veoveo concrete resource components](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/platform/types/DESIGN.md#concrete-resource-components) | All resource addresses use the shared URL 2.5.8 and percent-encoding 2.3.2 profile with Time's route, ID, and cursor validation. |
| [URI Template RFC 6570](https://www.rfc-editor.org/rfc/rfc6570) | Discovery declares simple ID variables and reserved expansion for slash-separated zone keys. Collection pages continue through a `cursor` query parameter on the collection root. The server parses concrete addresses through its typed resource contract. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; schedule expansion and timeline validation use durable, resumable task operations. |
| [Veoveo knowledge source extension](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/ext-knowledge/DESIGN.md) | `ai.veoveo/knowledge-source` declares documentation and Time-owned collections, typed source observations, conditional reads, and enumeration from each collection root. Events use Store-backed change notifications. |
| [Veoveo server registration](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/ext-registration/DESIGN.md) | Time registers with the Gateway through OAuth 2.0 Dynamic Client Registration (RFC 7591) and renews and removes it through RFC 7592. It discovers the registration endpoint from RFC 8414 metadata and advertises `ai.veoveo/registration`, `ai.veoveo/authorization` and `ai.veoveo/module` in MCP Discover. |
| [JWT access tokens RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html) | The Gateway forwards each admitted request with an EdDSA-signed access token whose RFC 9396 `authorization_details` carry the caller's granted Time permissions. Time verifies the token against the issuer and keys its registration returned. |
| [Problem Details RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html) | Administrative errors use `application/problem+json` with `https://veoveo.ai/problems/<code>` types and the `retryable` and `correlationId` extension members. |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | UTC and numeric-offset timestamp input and canonical UTC output, including explicit leap-second handling. |
| [RFC 9557](https://www.rfc-editor.org/rfc/rfc9557.html) | Timestamp input with an IANA time-zone annotation and explicit ambiguity policy. |
| IANA Time Zone Database and [TZif RFC 8536](https://www.rfc-editor.org/rfc/rfc8536.html) | Versioned civil-time authority, compiled release products, zone completion, and fold/gap resolution. |
| IANA `leap-seconds.list` | Versioned TAI-UTC transition authority. Every canonical instant binds its TZDB and leap-second releases. |
| SHA-256 digests | Requests, stored metadata and provenance use the foundational `Sha256Digest` spelling, `sha256:` followed by 64 lowercase hexadecimal digits. |
| TAI, UTC, TT, TDB, GPS, and Galileo system time | Typed projections from one integral TAI instant. The server records authority and uncertainty rather than treating scales as interchangeable strings. |
| [NTPv4 RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html) and [Network Time Security RFC 8915](https://www.rfc-editor.org/rfc/rfc8915.html) | Approved node clocks may use NTP/NTS. Time MCP consumes a bounded `ntpd-rs` observation; it does not act as an NTP network endpoint. |
| HTTPS | Registered IANA authority sources are acquired under fixed host, media, digest, size, and elapsed-time policy. |
| SurrealDB 3.3.0 and SurrealQL | Private runtime persistence uses Store's qualified Rust SDK version and connection. Bound parameters carry domain values; SQL applies catalog visibility, ordering, paging and activation transactions. Time owns the temporal schema lane. |

Time JSON uses camelCase members and snake_case vocabulary; decoding rejects
retired and mixed keys. Authority source is a closed tagged object: bootstrap
has no payload members, while acquisition carries its typed source and acquisition IDs.
The source variant order and admitted JSON spellings stay fixed. SQL, CLI/environment, prompt arguments, IDs and URI queries
keep their profiles. `ntpd-rs` retains upstream snake_case fields and PascalCase
leap values. Event cursors use envelope version 2 with `taiSeconds`/`eventKey`;
calendar, epoch and authority positions use envelope version 1. Knowledge hashes
current owner JSON and access.
Bootstrap identity and source digests hash raw TZDB/leap bytes, excluding JSON.

## Domain Contract

Time answers four operational questions:

- What physical instant does an expression identify under the active authority?
- How does that instant project into another zone or time scale?
- Which intervals and precedence constraints apply to an operation?
- Is the host clock good enough for the requested policy?

The canonical instant is `TimeInstant`:

```text
tai_seconds_since_1970
nanosecond
uncertainty_nanoseconds
authority.tzdb_release_id
authority.leap_seconds_release_id
```

Integral TAI seconds make ordering and interval arithmetic independent of civil
clock changes. Nanoseconds retain subsecond coordinates. The uncertainty field is
carried forward as evidence rather than folded into the timestamp. Authority ids
bind every instant to one TZDB release and one leap-second release.

`TimeWindow` is a nonempty half-open interval `[start, end)`. Its constructor and
JSON decoder require increasing nominal coordinates and one authority pair for both
bounds. Read-only accessors preserve those relationships after construction. This
convention makes adjacent shifts,
reservations, and routing windows compose without double-counting their shared
boundary.

## Portable Value Admission

Expression, projection, calendar, timeline, source and release construction and
JSON decoding share portable checks. GPS seconds are finite in [0,604800), Julian
and scale values are finite, and optional GPS week/seconds occur together. Local
calendar syntax, positive recurrence counts and intervals, point-name references
and separation bounds are checked without resolving a loaded authority. Expression
zone names keep their 128-byte profile; resource zone names keep their distinct
1024-byte profile. Fold, gap, leap and active-zone interpretation stay in the engine.

Source metadata shares its existing bounded printable fields and HTTPS profile
without credentials or fragments. Release paths use the existing absolute-path
profile and validation cannot precede retrieval. These checks confer no network or
file authority. Clock assessments agree with their violations; observation syntax
is portable while freshness, node trust and quorum are evaluated by the clock service.

Acquisition progress has a mutable representation. Its status, phase, staged-release
presence and creation/update order are checked on decoding, before catalog writes
and after selected lifecycle columns are applied. Queued work has no staged release;
running download/validation and cancellation retain their documented phases; only
successful completion claims a staged release. Failure and cancellation preserve
uncertain external outcomes according to the acquisition service. Current source
ownership and selected release existence are checked at use time.

Administrative errors are RFC 9457 problem details. `AdminProblemType` closes the
problem types, and `correlationId` is a canonical UUIDv7 correlation value rather than
a W3C trace identifier. The administration adapter owns HTTP status, retry and
redaction behavior.

## Architecture

### Library Features

The server library owns the public `contract` module. Consumers select
`default-features = false, features = ["contract"]` to reuse temporal IDs, DTOs,
authority references, the `TimePermission` vocabulary and the `TimeScope` enum. This
feature depends on `veoveo-types`, the registration declarations, Serde, Schemars, and
Chrono's date/time representation with clock support disabled. It includes no MCP transport, asynchronous runtime, database
client, acquisition engine, or provider dependency.

`runtime` adds the temporal engine, catalog, acquisition service, clock observation,
and application state. `mcp` adds the HTTP and MCP adapters and enables `runtime`.
The default feature is `mcp`; the binary requires it. Native catalog tests require
`runtime`. Contract consumer tests require only `contract`. The runtime still uses
shared task and identity contracts; only the contract feature promises dependency
isolation from those components.

All source, release, acquisition and event metadata carry `TimeVersion`, as do
immutable calendar and epoch versions. The type admits `1..=i64::MAX`, preserves the
numeric JSON representation and checks advancement for exhaustion. Source creation
uses `NewTimeSource` with the zero-only `SourceCreationVersion`; it cannot substitute
for persisted `TimeSource` metadata. `CreateSourceRequest` keeps its existing nested
fields and required numeric zero. Catalog creation publishes version one. Mutation
guards and metadata keep their types through catalog calls and event settlement.

`SubsecondNanoseconds` admits `0..=999999999` in instants, Unix/TAI expressions,
event cursors and persistence drafts. It serializes as the existing numeric field;
omitted expression fractions default to zero. Uncertainty and offsets describe
durations and keep their separate integer ranges. `TimeInstant::from_total_nanoseconds`
splits negative coordinates with Euclidean division and rejects a seconds value outside
`i64`. Epoch-relative resolution uses that constructor. Fractional numeric conversion
checks the floating-point upper endpoint and rounding carry before adding its epoch.
Protocol-library calls and database driver records receive raw integers at their adapters.
Leap-table NTP conversion checks epoch subtraction. TAI-to-UTC conversion selects the
offset directly from the ordered TAI transition intervals and checks the final subtraction;
it preserves representable early coordinates without guessing an offset from the newest entry.

`TimeAuthorityReference::new` takes a typed release URI, dataset kind, provenance,
digest and nonblank version label. The URI supplies the release ID. A private wire
adapter preserves the published fields and checks their repeated identity on decoding.
`EffectiveTimeAuthority::new` checks the two dataset roles and distinct release IDs;
its `binding` method derives the instant's `AuthorityBinding`. Both the binding and
the complete pair expose read-only accessors. `AuthorityContext` derives its private
binding from the checked pair and exposes read-only accessors for its metadata and loaded
time databases. Catalog reads select visible records in SQL and verify their
producing-acquisition relationships.

The schemas constrain each pair member's dataset kind and require a nonempty version
label. Decoding additionally checks URI/ID equality, distinct release IDs and nonblank
labels; those relational and whitespace checks use the Rust constructors. These metadata
checks describe reference consistency; authority selection and file loading run in the registry.

`ResolveTimeOutput::new` takes an instant, its complete authority pair and the engine's
`TimeProjection`. Construction and JSON decoding require the instant's binding to agree
with both release references. Read-only accessors protect the admitted pair. Projections
keep their existing flat wire fields; the schema describes that shape, while the constructor
checks release agreement. The contract library does not load authority files or recompute
UTC, GPS and Julian representations. The temporal engine owns those calculations.
`ConvertTimeOutput` embeds this checked resolution as its `canonical` field.

`TimeWindow` keeps the existing `start` and `end` wire fields. Its schema describes
each instant; the Rust constructor enforces ordering and authority agreement across
the fields. Runtime operations also check that this pair matches the active engine.
Interval uncertainty describes the selected nominal bounds and does not widen them.
The contract's intersection method clips two checked windows and returns no window
for disjoint or touching inputs. Equal endpoint coordinates keep the larger uncertainty.

Task recovery rejects an invalid persisted horizon and stops startup.

`TimePermission` declares Time's five permission names once through `Vocabulary` and
implements `PermissionDefinition` with each permission's resource, access and risk.
`TimeScope` declares the single coarse OAuth scope `time`, which knowledge collections
require of their readers. [Authorization](#authorization) describes how handlers use them.

`mcp/setup.rs` implements `McpServerContract` with `TimeScope` and `TimeResource`.
The HTTP process validates its `McpServerSetup` before serving. MCP initialization
and resource discovery consume that setup. Resource descriptors, including the
embedded documents, are built from typed addresses. The setup preserves Time's reserved
zone template; the owning resource tests qualify routes.

Template descriptors use `McpResourceTemplate` and the foundational RFC 6570 type.
Native contract tests expand every Time route declaration through iri-string and
compare the result with `TimeResource` construction. Reserved zone names preserve
slashes and plus signs. Collection routes bind an optional `cursor` query and cover
each typed cursor envelope, while discovery lists the collection roots as resources.
Concrete resource reads continue through the same owning parser.

`TimeResource` owns every resource route and implements `ResourceAddress`. Its variants
carry the corresponding ID, version, zone key, or collection cursor. Its derived
route declarations drive parsing, typed component building and checked discovery
templates. Time owns codecs for documents, versions, slash-separated zone keys and
opaque cursors; cursor codecs preserve the existing hex-encoded JSON envelope. Reads
and subscriptions use this same contract. Unsupported parameters, fragments, encoded
ID aliases, relative paths, and incorrect ID families are rejected before dispatch.
`TimeAuthorityReleaseUri` restricts provenance fields to acquired or packaged authority
references and requires an `AuthorityReleaseId` when constructed. The reference builder
checks that the URI variant agrees with the source kind.

`TimeVersion` validates the positive signed database range. `TimeZoneId` validates a
relative TZDB key with nonempty ASCII name components and a maximum of 1024 bytes;
the active authority determines whether that zone exists. It preserves slash-separated
names and literal plus signs in the public resource path.

`CalendarCursor`, `EpochCursor`, and `EventCursor` retain their family's typed IDs and
validate their current envelope revision, position, and collection name during deserialization. Page
responses serialize these types as opaque strings. Catalog methods require the matching
cursor type; admin query extraction and event recovery use those same types. The
contract feature includes Serde JSON and hex for this wire format.

Temporal IDs validate their domain prefix, length, and character set during JSON
deserialization as well as construction. `TimeAccessContext` carries tenant and
principal selection for database access; it is distinct from an authorization scope.

### Hosted Process

```text
agent
  |
  | MCP and Task API
  v
Gateway
  |
  | RFC 9068 access token with granted Time permissions
  v
veoveo-mcp-time
  |-- temporal engine
  |-- MCP resources, tools, prompts, completions, and subscriptions
  |-- durable task adapter
  |-- administrative REST
  |-- IANA acquisition and zic compilation
  |-- ntpd-rs observation adapter
  |-- SurrealDB platform store
  `-- persistent authority release volume
```

The Rust process owns the HTTP server, acquisition jobs, active authority cache,
event watchers, and Task API workers. The same image contains `zic`, the bootstrap
TZif database, and the bootstrap IANA leap-second file supplied by Debian `tzdata`.

## Authority Data

Time uses two independently versioned authority families.

| Family | Authoritative content | Runtime product |
|---|---|---|
| `tzdb` | IANA Time Zone Database source release | compiled TZif directory |
| `leapSeconds` | IANA `leap-seconds.list` | validated TAI-UTC transition table |

An installation begins with the TZif and leap files in the image. Their bootstrap
release IDs bind the authority family and SHA-256 digest of the packaged source file.
`server/bootstrap.rs` constructs their references through the typed URI builder.
The same bytes keep their identity across restarts and file relocation; changed bytes
receive a different identity. Tenant administrators can register
HTTPS sources, acquire new content, inspect staged releases, and activate one
release per family.

An `AuthorityBinding` selects the active pair. Activating one family preserves the
current release of the other family. The engine reloads the pair as one context,
which keeps civil and physical projections coherent.

Every effective family also carries a compiler-ready `TimeAuthorityReference`.
The reference names its immutable acquired-release or packaged-bootstrap URI,
release id, dataset kind, version label, and canonical SHA-256 digest. Acquired
releases name the registered source and producing acquisition. Image-provided
authorities use the explicit `bootstrap` source kind and a digest of the packaged
source file. Consumers can therefore admit a result from its returned references
without searching the authority catalog.

Release and acquisition metadata, acquisition requests and persistence drafts carry
digests as the foundational `Sha256Digest`, whose one spelling is `sha256:` followed
by 64 lowercase hexadecimal digits. Only driver records hold digest strings. Download
verification, acquisition idempotency and provenance compare that one spelling, and
diagnostics do not quote a rejected digest. Catalog reads check each release digest
and each present acquisition digest even when the JSON body matches the indexed value.

### Acquisition Flow

The administrative acquisition job runs inside the Time process:

1. Resolve the enabled registered source under the authenticated tenant.
2. Download over HTTPS with redirects disabled, a response deadline, a media-type
   check, and a byte limit.
3. Compute SHA-256 while streaming to the private acquisition workspace.
4. Compare an optional expected digest before interpreting the content.
5. Validate leap data or safely extract and compile a TZDB source archive with
   `zic`.
6. Load representative TZif records or parse the leap table.
7. Move the product into an immutable release directory and create a staged
   release record.

Archive extraction accepts regular files and directories. Absolute paths, parent
traversal, device entries, links, and expanded content beyond the configured limit
are excluded from the product.

Acquisition idempotency is scoped to tenant, principal, and idempotency key. A
repeated request returns its existing job when source and expected digest match.
Cancellation produces the terminal `cancelled` state and removes scratch content.

### Activation

Activation uses two checks before publication:

- optimistic versions for the staged release and active family pointer;
- a full load of the prospective TZDB and leap-second pair.

The SurrealDB transaction requires the candidate's stored key, tenant, family,
staged state and expected version. Pointer replacement rechecks its tenant, family,
current release, previous-release history and version. Retirement requires the
selected previous release's key, tenant, family, active state and observed version.
A failed check rolls back the candidate, pointer and retirement together. The
transaction marks the candidate active, advances the family pointer and retires
the superseded release. The process then replaces the tenant engine and
notifies subscribers of `time://authorities/current`.

Authority release records retain source id, source URL, SHA-256 digest, retrieval
time, validation time, version label, artifact path, lifecycle state, and optimistic
record version.

`catalog/activation.rs` prepares a private draft containing the staged candidate
and the complete admitted pointer/release snapshot. The registry loads the prospective
pair from that draft under its 30-second deadline, then consumes the draft to publish.
The public runtime activation method always performs file preflight.
The commit compares both families and the candidate with their observed metadata;
an absent pointer is part of that comparison. Changed metadata rejects publication
even when an external repair did not advance the version. SQL compares optional
pointer history by value because a missing stored field and a driver `NONE` have
different object representations.

Every activation registers both exact `time_active_authority` IDs for commit-time conflict detection with
`SELECT … FOR UPDATE`, including absent pointers. It registers the candidate and
both preflight release records too, then compares the complete preflight snapshot before
changing a release or pointer. IDs are read in stable order. The database takes no blocking row lock. A concurrent insert,
update or deletion invalidates the transaction under the selected SurrealDB 3.3.0
profile. A failed activation rolls back every write. Clients load the current pair
again before retrying; the server performs no automatic mutation retry.
Store's transaction-error selector preserves the causal database error instead of
reporting an earlier cancelled statement as the cause.

The schema contains no activation-fence table. Native qualification covers
different-family contention starting with absent pointers, existing pairs, concurrent
pointer and release repairs, stale preflight metadata, tenant mismatch and failed file
loads.

### Tenant Authority Contexts

Every engine request reads the active tenant pair through the joined catalog query
and resolves each selected release's acquisition provenance. This requires one
selection query and at most two provenance reads; authority-only requests do not
load epochs. Event-page recovery shares one validated context across the
page and skips events with existing watchers. A family without a stored pointer uses its packaged bootstrap
reference. Invalid visible metadata and database errors fail the request.

The registry caches loaded contexts under the Store's typed tenant identity. A hit
requires the current effective references, provenance and artifact paths to match
the cached inputs. An engine gets its own epoch map while sharing the loaded
authority data. LIVE and reconciliation signals evict process contexts before resource
notifications; a generation token prevents an earlier loader from repopulating an
evicted cache. Request-time catalog checks also cover delayed or disconnected
notification delivery.

Selection and file loading share a 30-second deadline. TZDB loading runs on a blocking
worker, and async leap-file reads preserve the runtime's responsiveness. A failed load
or explicit reload removes the tenant's cached context. Exact authority-release
metadata reads use the packaged reference or the tenant-scoped release catalog and
can support diagnosis when the current engine cannot load.

## Accepted Time Expressions

`resolve_time` accepts a tagged `TimeExpression`.

| Format | Contract |
|---|---|
| `rfc3339` | UTC or fixed-offset timestamp |
| `rfc9557` | timestamp with an IANA zone annotation and explicit disambiguation |
| `civil` | local date-time, IANA zone, TZDB release id, and fold/gap policy |
| `unix` | seconds and nanoseconds since the Unix epoch |
| `tai` | TAI seconds and nanoseconds since 1970-01-01 TAI |
| `gps` | GPS week and seconds of week |
| `julian_tai` | Julian day in the TAI scale |
| `militaryDtg` | `DDHHMMZMONYY` or `DDHHMMSSZMONYY` with a NATO zone letter |
| `epoch_relative` | epoch id and signed nanosecond offset |

Civil folds and gaps default to `reject`. Callers select `earlier` or `later` when
the operation has an explicit policy. Military zone `J` requires an IANA zone and
is rejected by the DTG parser because `J` denotes local time rather than a fixed
offset.

Resolution returns the canonical instant together with UTC RFC 3339, an explicit
`utcIsLeapSecond` flag, military DTG, Unix seconds, GPS week/seconds when the
instant follows the GPS epoch, and Julian TAI day. A positive leap second keeps its
`:60` representation instead of collapsing onto an adjacent UTC second. Resolution
and conversion return the two effective authority references. They do not attach a
current clock observation because supplied-instant conversion is deterministic.

`convert_time` validates the instant's authority binding before projecting it. It
can return selected IANA zoned values and UTC, TAI, TT, TDB, GPST, and GST scale
representations.

## Operational Calendars

An `OperationalCalendar` is immutable by `(calendar_id, version)`. It declares one
IANA zone, local windows, daily or weekly recurrence, optional weekday filters,
count or until bounds, excluded civil dates, and caller-defined labels.

`expand_schedule` resolves the local windows through the active TZDB and clips them
to an authority-bound horizon. The output is ordered, numbered, and bounded by
`maximumOccurrences`. Expansion understands offset changes because every occurrence
is resolved from its local civil time rather than by adding fixed UTC durations.
Clipped bounds retain the selected horizon endpoint's uncertainty. A bound shared with
an occurrence keeps the larger uncertainty, and an occurrence touching only the
horizon's exclusive edge is omitted. Recurrence count and cutoff apply to the original
occurrences before clipping; labels carry through to the numbered results.

The operation accepts at most 1,000,000 returned occurrences and uses a bounded
calendar search horizon. Invalid local times remain explicit errors unless the
calendar chooses a supported unambiguous time.

## Epochs And Timelines

An epoch gives a named physical instant a version. Agents can then resolve
expressions such as an offset from launch, H-hour, or the start of a convoy window.
Each calculation forks the tenant's authority engine and loads only the epoch ids
named by its input expressions. SurrealDB selects the latest version of each key
inside the caller's tenant, in batches of at most 100 keys. The request has a
30-second epoch-read deadline and accepts at most 100,000 expressions. A calculation's
epoch set belongs to that calculation, so concurrent requests cannot replace it.
Engine maps keep `EpochId` keys. Relative resolution requires the epoch's
authority pair to match the active engine and preserves its uncertainty while adding
the requested offset. Additional resolution uncertainty uses checked addition; a sum
outside the unsigned 64-bit nanosecond range fails. Activation may therefore require
publishing a new epoch version under the selected authority before relative calculations
can resume. Exact reads keep the stored epoch's original binding.
An exact epoch resource read selects its tenant and epoch key in SurrealDB, orders
versions descending, and fetches one record. Clock observations, exact resource reads,
and zone completions use the authority cache without loading the epoch catalog.

`validate_timeline` resolves named points and evaluates directed constraints. Each
constraint identifies a predecessor, successor, minimum separation, and optional
maximum separation. The result reports every violated constraint by input index.

Timeline evaluation accepts up to 100,000 points and 1,000,000 constraints in one
task. Point names must be unique, referenced points must exist, and a maximum cannot
be below its minimum.

## Window Algebra

`evaluate_windows` calculates union, intersection, and difference over authority-
bound half-open windows. Adjacent ranges coalesce during union. Every bound in one
request must use the active authority pair, which makes the result suitable for
direct use by routing, scheduling, and optimization tools.
Each output endpoint retains the uncertainty at its input coordinate. When several
input bounds share that coordinate, the result carries their maximum uncertainty.
Difference preserves this metadata on newly exposed cut boundaries as well. The
operations compare nominal coordinates and do not expand ranges by uncertainty.

## Clock Quality

Clock synchronization belongs to node infrastructure. An installation can run
`ntpd-rs` with NTS, hardware timestamping, PTP-backed sources, GNSS, or another
approved reference architecture. Time consumes the resulting observation and
turns it into agent-visible evidence.

The implemented adapter reads one bounded JSON observation from an ntpd-rs Unix
socket. It projects:

- synchronized state and stratum;
- estimated offset and conservative error bound in nanoseconds;
- independent source count;
- holdover age when the adapter supplies it;
- NTP/NTS traceability labels and observation time.

`assess_clock` compares the observation with an explicit request policy or the
tenant's stored default. Policy controls maximum error, maximum stratum, minimum
source diversity, and maximum holdover age. An installation without an observation
socket reports an unmeasured system clock with an unbounded error estimate.

`ClockQualityPolicy` exposes a named builder and read-only accessors. JSON decoding
uses the same validation. Maximum error in nanoseconds and maximum holdover in
seconds must lie in `1..=i64::MAX`; maximum stratum is `1..=15`, and minimum source
diversity is `1..=u32::MAX`. The published schema declares those bounds. Assessment
requests and persisted policies share this contract.

The readiness probe reports ready while Time observes the configured clock. Mission acceptance remains a policy decision returned by `assess_clock` and
`time://clock/quality`. The clock-derived `time://clock/current` resource includes
the effective policy and measured quality, including holdover age.

## MCP Surface

### Tools

| Tool | Permission | Execution | Result |
|---|---|---|---|
| `resolve_time` | `time.authorities.read` | direct | canonical instant and projections |
| `convert_time` | `time.authorities.read` | direct | selected zone and scale projections |
| `assess_clock` | `time.authorities.read` | direct | clock observation, policy, and violations |
| `evaluate_windows` | `time.schedules.expand` | direct | normalized interval set |
| `expand_schedule` | `time.schedules.expand` | Task API required | bounded calendar occurrences |
| `validate_timeline` | `time.timelines.validate` | Task API required | constraint verdict and violations |
| `create_event` | `time.events.write` | direct | owner-scoped event |
| `cancel_event` | `time.events.write` | direct | cancelled event version |

Every successful tool result contains typed structured content. Tool annotations state
read-only, destructive, idempotent, and open-world behavior and agree with each tool's
permission. Cancelling an event keeps the event and its history, so `cancel_event` is
not destructive.

### Task API

Schedule expansion and timeline validation use the final Task API extension. A normal
tool call returns an instruction to invoke the task form. The Task adapter supports
create, get, update, cancel, list discovery, and task subscriptions.

Tasks persist through `veoveo-task-runtime` in SurrealDB. Ownership includes tenant,
principal, profile, server, and data labels. Workers claim 120-second leases and renew
them every 40 seconds. Both temporal task types use `Resume` recovery because their
outputs are deterministic under the persisted authority-bound request. Terminal task
records retain for seven days unless a retention pin extends their lifetime.

Workers read durable cancellation before scope resolution, after scope resolution,
and after authority and epoch loading. These checkpoints observe requests committed
through another server replica without relying on a shared local cancellation token.
Hosted schedule expansion and timeline validation run finite calculations on Tokio's
blocking pool. The registered async worker retains the original job outside its
cancellable waiter and awaits that job before normal exit, including after a failed
lease heartbeat. Cooperative stop checks occur at calendar windows, recurrence
days, timeline points and constraints, and output indexing. Public engine methods
stay synchronous for library callers. Sorting and serialization finish between
checkpoints; this profile promises neither hard abort nor a fixed drain latency.
Hard process termination follows the existing Resume lease profile.
A local shutdown stop without durable cancellation prevents result publication and
leaves the current lease and request for `Resume` recovery; it does not fabricate a
terminal cancellation. Success publication carries that token through snapshot
selection and checks it immediately before entering each shared transition. A stop
observed after selection reads current state once to settle durable cancellation or
leave the unfinished request for recovery. Entering the shared transition starts
settlement: its internal database awaits and compare-and-set determine the outcome.
A later local stop cannot undo that dispatch, and shutdown may leave its outcome
unresolved until current-state reconciliation. Local token cancellation and database
settlement do not form an atomic operation.

Final calculation settlement delegates the selected Task snapshot to TaskRuntime's
Resume settlement with the PreserveFailure policy. If cancellation wins that transition, the executing
worker rereads current state once and settles `CancelRequested` as `Cancelled`
without publishing the calculated result. An already committed terminal outcome
wins. A genuine calculation failure keeps its failure code and message, including
when cancellation precedes its settlement. The worker checks its current, unexpired
lease before attempting a write; the runtime also enforces its transactional lease
and snapshot guards. Unrelated progress conflicts and storage errors stop execution
without being reported as successful settlement. This path performs no polling.

Startup recovery observes the finite set of retained Tasks.
The shared observer applies the initial report before HTTP starts and revisits live
leases through SQL change notifications and lease deadlines. Each resumable request
passes the existing type and claim guards. A failed claim skips local scheduling
only when current SQL proves physical absence, terminal settlement, or a matching
Task held by another worker's unexpired lease. An unproven conflict or recovery
observation error stops HTTP serving.

The installed schedule profile, kept in the monorepo's
[installed harness](https://github.com/BiomaAI/veoveo/blob/0c33cf8a7e5a1d46f953702553ac6643f99c1363/servers/time-mcp/tests/gateway_consumers/schedule.rs), selects
completion, cancellation or unfinished process recovery explicitly. Cancellation
and recovery require both delivered and current Working state for the acknowledged
opaque Gateway Task identity. Recovery admits one maintained Rust container, observes
its externally signalled exit and same-Pod replacement, then requires the same Task
still Working before accepting delivered Completed and its independently expected
result. A completed Task surviving replacement does not qualify unfinished recovery.

The fixture sets one operation deadline before connecting: 120 seconds for completion
or cancellation and 300 seconds for recovery. Recovery needs room for the worker's
120-second lease, renewed every 40 seconds, to expire before another worker resumes
computation. Every nested wait ends by that operation deadline. The scenario grants
360 seconds of execution and a separate 30-second owner cleanup interval. Actual
listener, caller and crash-watch handles keep their original consuming close futures
and deadlines. The fixture never signals a process itself; Ops must wait for its
persisted armed Task/watch marker. A selected historical UTC daily recurrence can
scan up to one million dates per window while returning a small independent output.
No duration is guaranteed, and an early terminal observation fails qualification.

### Zone Completion

Time advertises `time://zones/{+zone_id}` to preserve slash and plus signs in zone
keys. Completion requests use that exact advertised template after the
`time.authorities.read` check. Clients obtain the reference through `resources/templates/list`; other template
spellings do not select the zone completion handler. Contract and discovery tests
qualify the current template against the typed zone builder.

### Knowledge Collections

Time declares calendar versions, epoch versions, events, acquired authority releases
and packaged bootstrap authorities through `ai.veoveo/knowledge-source`. Every domain
collection requires the coarse `time` scope and enumerates from its collection root,
such as `time://events`. Collection pages contain at most 100 `items`, each with a
typed `uri` and `title`, and an optional `nextCursor`. Source SQL selects tenant and event owner before ordering and LIMIT.
Every epoch version has its own member URI; the unversioned epoch resource selects
the current version for operational callers.

Calendar, epoch and acquired-release observations carry the stored tenant, creating
principal and Work Context with `readPolicy: {kind: "tenant"}`. Events use `subjects`
and remain readable only by their owner. A reader's selected context never supplies
provenance. The source checks logical identities against the native ownership links.
Revisions bind the returned JSON, its access descriptor and stored modification time.
The creating principal comes from that same access descriptor. Conditional reads
follow the same SQL admission as full reads and use private zero-TTL delivery.

The packaged bootstrap collection uses profile access and carries no invented record
owner or context. Its member addresses use `time://authorities/bootstrap/{release_id}`;
tenant-acquired references use `time://authorities/releases/{release_id}`. The typed
reference builder rejects disagreement between source kind and resource address.
Both kinds retain their original compiler digests. Calendar and epoch versions and
compiler references are immutable. Event observations include the current stored
modification time, and event changes use the existing Store-backed subscriptions.
Events do not claim a last-modifying principal because scheduler transitions record
no actor. Immutable members attribute creation to their stored owner. Member JSON is
limited to 64 KiB; calendar creation rejects a larger document before writing it.

The monorepo's [`gateway_source_conformance.rs`](https://github.com/BiomaAI/veoveo/blob/0c33cf8a7e5a1d46f953702553ac6643f99c1363/servers/time-mcp/tests/gateway_source_conformance.rs) supplies two future events to the shared MCP
checker through the public gateway. It cancels one, restarts the Time Deployment,
then cancels the second. The checker verifies both notification paths, the retained
cancellation and the unchanged second event across restart. Cleanup reconciles both
events to cancelled state. Every declared collection needs populated fixtures.
Inputs and commands follow [the installed harness contract](https://github.com/BiomaAI/veoveo/blob/0c33cf8a7e5a1d46f953702553ac6643f99c1363/testing/installed/DESIGN.md).

### Resources

| URI | Content |
|---|---|
| `time://clock/current` | current resolved instant with effective policy and measured clock quality |
| `time://clock/quality` | measured clock-quality record |
| `time://authorities/current` | effective compiler-ready authority references, including bootstrap authorities |
| `time://calendars` | a page of tenant calendar versions |
| `time://epochs` | a page of tenant epoch versions |
| `time://events` | a page of owner-scoped events |
| `time://authorities/releases` | a page of tenant-acquired authority releases |
| `time://authorities/bootstrap` | a page of packaged authority releases |

`resources/list` advertises the stable roots, collection roots and documents. Domain
records are reached through the collection pages and exact URI templates. Changes to
domain records invalidate resource contents; the discovery inventory is static and does
not advertise resource-list change notifications.

Calendar, epoch, event and authority pages contain `items`, `limit: 100`, and
`nextCursor`. A client reads the next page by passing the opaque cursor as the root's
`cursor` query parameter, as in `time://events?cursor=<nextCursor>`. SurrealDB
applies authorization and the cursor position before its 101-record lookahead.
Calendars and epochs sort by key ascending and version descending. Events sort by
TAI seconds, nanosecond, then event key ascending. These positions use immutable
fields. Each page observes the records present at its read. Inserts before its position
appear on a fresh walk. The administrative calendar and epoch lists use the same
pages and optional cursor query parameter.

Subscribe to the collection root to invalidate its pages. Cursor URIs do not accept
separate subscriptions. A client that pages a collection rereads its current page
after a root notification.

Resource templates expose:

```text
time://docs/{doc_id}
time://zones/{+zone_id}
time://authorities/releases/{release_id}
time://authorities/bootstrap/{release_id}
time://calendars/{calendar_id}/versions/{version}
time://epochs/{epoch_id}
time://epochs/{epoch_id}/versions/{version}
time://events/{event_id}
```

Completions enumerate the packaged IANA zones and visible calendar, version,
epoch, and event ids. Store-backed completion queries apply tenant and event-owner
predicates, text matching, distinct selection, and ordering in SurrealDB before
fetching at most 101 candidates. MCP returns 100 values and sets `hasMore` when a
further candidate exists. It reports a total only when the complete match set fits.
Calendar-version completion narrows its SQL query to the selected `calendar_id`
when the client supplies that argument in completion context.
Calendars, epochs, authorities, clock quality, and events emit resource updates.
Reading an event or an event page restores its scheduled watchers. An event-root
subscription restores all of its owner's scheduled events through SQL-filtered
pages, with a 60-second deadline. A timeout rejects listener establishment instead
of claiming complete recovery. Exact event subscriptions restore that event's
watcher. Resource discovery does not start watchers.

### Prompts

| Prompt | Purpose |
|---|---|
| `resolve_operational_time` | guide authority-aware normalization |
| `expand_operational_calendar` | prepare a bounded Task API expansion |
| `validate_mission_timeline` | prepare named points and separation constraints |

## Events

Events are owner-scoped durable records. Creation validates an authority-
bound due instant and uses a principal-scoped idempotency key. A watcher transitions
the record from `scheduled` to `due` under optimistic concurrency, then emits updates
for the collection and event URI. Cancellation updates the durable record and cancels
the local watcher.

The installed future-event consumer arms the stable event-root subscription before
creation and reads the current authority-bound clock to construct checked TAI
coordinates. Delivered invalidations require matching uncached event reads;
Scheduled and Due are separate assertions. One fenced process restart retains the
future event, and reconnect restores its watcher. A deliberately disconnected
interval qualifies current-state reconciliation rather than notification replay.
The profile owns one 300-second operation and 30-second cleanup interval, retains
ambiguous creation intents, and cancels only acknowledged owned Scheduled events.
Due and cancelled event records remain for operator retirement. Agent wake scheduling uses
these public resources through the agent's own owner identity and policy.

Event reads, collections, due queries, and optimistic transitions bind both the
tenant and authenticated owner in SQL. Completion applies the same owner predicate
before its limit. The catalog decodes only the records selected by those queries;
notification scheduling uses those owner-scoped reads.

## Administrative HTTP Projection

The administrative HTTP surface serves installation authority workflows. It reuses
the same typed temporal models, catalog, and task state as the MCP surface and is not
a second authority. Time serves it at `/time/admin/{path}` behind the same forwarded
access token check as MCP, and every route requires `time.authorities.administer`.

| Method and path | Operation |
|---|---|
| `GET /sources` | list registered authority sources |
| `POST /sources` | create a source |
| `GET /sources/{source_id}` | read a source |
| `PUT /sources/{source_id}` | replace a source under optimistic concurrency |
| `GET /acquisitions` | list acquisition jobs |
| `POST /acquisitions` | start an idempotent acquisition |
| `GET /acquisitions/{acquisition_id}` | read acquisition progress |
| `POST /acquisitions/{acquisition_id}/cancel` | request cancellation |
| `GET /releases` | list authority releases |
| `GET /releases/{release_id}` | read release provenance |
| `POST /releases/{release_id}/activate` | preflight and activate a staged release |
| `GET /active-authorities` | read active family pointers |
| `GET /calendars` | list calendar versions |
| `POST /calendars` | create an immutable calendar version |
| `GET /calendars/{calendar_id}/versions/{version}` | read a calendar version |
| `GET /epochs` | list epochs |
| `POST /epochs` | create an epoch version |
| `GET /epochs/{epoch_id}` | read the latest epoch version |
| `GET /clock-policy` | read the tenant clock policy |
| `PUT /clock-policy` | replace clock policy under optimistic concurrency |

`GET /active-authorities` returns `AdminPage<ActiveAuthoritySelection>`. Each
closed entry contains `pointerVersion` and `release`, an admitted active
`AuthorityRelease`. The positive pointer version advances independently of the
release's `recordVersion`. An operator uses the selection's `write_guard()` as
`ActivateReleaseRequest.expected_active_pointer_version`; the method constructs
`TimeWriteGuard::Existing(pointerVersion)` rather than using the release version.

The catalog reads pointers and releases in one joined statement and admits their
tenant, key, family, active state and stored body relationships. A visible invalid
pointer fails the read. An omitted family has no persisted pointer and uses
`TimeWriteGuard::Absent` for its first activation; an empty page means neither
family has a persisted pointer. Bootstrap effective references belong to
`time://authorities/current` and do not invent persisted pointer versions.

Administrative clients decode the closed selection shape.

Administrative errors are `application/problem+json` documents with `type`, `title`,
`status`, `detail`, `retryable`, and `correlationId`. The type is one of
`https://veoveo.ai/problems/invalid-request`, `forbidden`, `not-found`,
`version-conflict` or `internal-error` under the same base. A request without the
administrative permission receives `403` with the `forbidden` type, and its detail
names the missing permission.

## Persistence

SurrealDB is the canonical temporal catalog and task store.

The runtime's private `persistence/` modules own temporal queries, driver records,
mutation drafts and validation. `PlatformStore` supplies the connection and platform
identity. Time owns `schema/migrations/` and its module lane. Time uses the same pinned SurrealDB 3.3.0
SDK as Store, behind its `runtime` feature. Contract-only consumers do not resolve it.

Catalog calls retain source, release, acquisition, calendar, epoch and event ID types
through the persistence interface. Calendar, epoch and existing-record guards use `TimeVersion`.
Collection queries accept `CalendarCursor`, `EpochCursor` or `EventCursor` directly;
requested epoch batches and completion parents also retain their domain IDs. The
driver binds their text and numeric values alongside typed database identity records.
Production statements live in `src/persistence/queries/`; native fixture statements
live in `src/tests/queries/`. Each call embeds a named file and binds its inputs.
Completion selects one of six fixed queries through `TimeCompletion`. Authority
activation embeds one complete transaction, including exact-ID locks, the joined
pointer/release snapshot and optimistic updates. The standalone active read and
transaction snapshot use the same predicates and ordering.
Tenant and event-owner predicates, sorting, grouping and limits execute in SurrealDB.

Public Time IDs accept bounded prefixed names because bootstrap authority provenance
can name image-provided releases. Stored catalog keys require the corresponding
prefix followed by a UUID whose version is 7, preserving the existing persistence
profile. Public syntax does not establish that a stored record exists. Bootstrap
references keep their public spelling and are not converted into database keys.
The persistence admission check applies before stored reads and writes. It does not
narrow public provenance deserialization or rewrite retained keys. Time's schema lane
defines tables and record fields; the Time contract defines JSON bodies and cursor versions.

| Table | Responsibility |
|---|---|
| `time_source` | registered IANA source endpoints and media policy |
| `time_authority_release` | immutable release provenance and lifecycle |
| `time_active_authority` | one optimistic pointer per tenant and data family |
| `time_acquisition` | idempotent acquisition state and progress |
| `time_calendar_version` | immutable operational calendars |
| `time_epoch` | versioned named physical instants |
| `time_event` | owner-scoped scheduled events |
| `time_clock_policy` | tenant clock acceptance policy |

All tables are schema-full and carry a 30-day changefeed. The Time module lane creates
their fields and indexes during installation. The server connects with the database
user its Gateway registration returns and never applies migrations.

### Version Guards

`TimeVersion` admits positive integers through `i64::MAX`. Source replacement,
event transitions and release activation require it. Clock-policy replacement and
active-pointer admission use `TimeWriteGuard::Absent` or `Existing(TimeVersion)`;
serialization keeps the established numeric zero/positive representation. Source
creation keeps its separate `record_version: 0` request sentinel. These request
schemas reject values outside their storage range before dispatch.

Every lifecycle increment checks the storage maximum. Exhaustion fails the write
without wrapping, resetting or changing the stored value. Release activation checks
both proposed versions before dispatch. Retirement increments the previous release
inside that transaction with a positive, below-maximum version predicate; failure
rolls back the candidate release and pointer together.

### Acquisition Phases

`TimeAcquisitionPhase` owns queued, downloading, validating, complete, cancelling,
cancelled and failed spellings across the public contract, worker, driver and schema.
The database rejects unknown phases before changing a row; contract-only consumers
use the same vocabulary without database dependencies. Status and phase keep their
separate transition responsibilities.

### Retained Catalog Metadata

`catalog/records.rs` converts SQL-selected rows into public values. It checks each
stored UUID key against the typed ID in the JSON body and the physical record ID.
Calendar and epoch keys include their positive version. Event and epoch nanoseconds
must be below one billion. SQL applies tenant and event-owner visibility before this
conversion; a visible inconsistent record fails its read or page without being dropped
from the results. Reads never repair or rewrite retained data.

| Entity | Body fields that must agree with the row | Values taken from lifecycle columns |
|---|---|---|
| Source | ID, name, dataset kind, URL, content type, enabled flag | record version |
| Authority release | release and source IDs, dataset kind, version label, URL, digest, artifact path, retrieval and validation times | state, record version |
| Acquisition | acquisition and source IDs, expected digest | status, phase, staged release ID, record version, update time |
| Calendar | ID, version, name, zone | none |
| Epoch | ID, version, name, TAI seconds and nanosecond | none |
| Event | ID, name, due TAI seconds and nanosecond | state, record version |

Lifecycle columns can advance while a valid JSON body keeps its earlier state and
version. Retirement uses this rule for release state and version. Public contract
models decode every body; zero, overflowing, missing and nonnumeric versions fail.
Calendar and epoch versions describe immutable identities and must agree with their
columns. Acquisition updates preserve creation time, source and expected digest,
and SQL version predicates fence competing updates. Diagnostics identify the entity
and field without quoting stored content.

`catalog/clock.rs` checks the physical clock-policy ID and tenant, converts stored
signed scalars without narrowing, and admits the policy and version through their
contract types. Invalid scalar diagnostics name the field without quoting stored
values. Reads leave rejected rows unchanged.

Every stored instant fraction must be within `0..=999999999`. Each instant binding
and packaged bootstrap pair must use distinct release IDs for TZDB and leap seconds.
Reference decoding checks repeated URI/ID agreement, source location, dataset roles
and nonblank labels. The public instant and Unix/TAI schemas declare the same
subsecond range. Zero-version update requests apply only to an absence guard.

Time decodes only the current body format and supplies no historical-body adapter.
Changing a stored body format or a bootstrap identity starts from a fresh Store. Epochs
and events that reference a withdrawn packaged authority are recreated rather than
reinterpreted under a new release. Acquired authority releases keep their stored
identities and artifacts. Bootstrap IDs are derived by the server and have no
configuration override or alias.

`persistence/active.rs` selects tenant pointers and resolves their releases within
one SQL statement. The release subquery applies tenant, family, key and active-state
predicates. A visible pointer without a matching release fails the read; only an
absent pointer permits bootstrap selection during reload. Pointer admission checks
its physical tenant/family key, positive version, stored release-ID syntax and
previous-release history. Catalog decoding then checks the selected release body.
These checks leave stored records unchanged and apply after every restart.

Compiled authority products live under `/var/lib/veoveo/time/releases`. Acquisition
scratch data lives under `/var/lib/veoveo/time/acquisitions` and is removed at terminal
completion. Time's registration key lives under `/var/lib/veoveo/time/keys`. A
deployment mounts `/var/lib/veoveo/time` from a persistent volume.

## Authorization

The Gateway grants permissions, and Time checks every request against them again.
`TimePermission` names five permissions in Time's namespace:

| Permission | Resource | Access | Grants |
|---|---|---|---|
| `time.authorities.read` | `time.authority` | read | `resolve_time`, `convert_time`, `assess_clock`, every resource and template, prompts, resource listing and completions |
| `time.schedules.expand` | `time.schedule` | read | `evaluate_windows` and the `expand_schedule` Task |
| `time.timelines.validate` | `time.timeline` | read | the `validate_timeline` Task |
| `time.events.write` | `time.event` | write | `create_event` and `cancel_event` |
| `time.authorities.administer` | `time.authority` | write, administer risk | every administrative HTTP route |

`mcp/requirements.rs` holds one requirement table that maps each tool, fixed resource,
resource template and prompt to its permission. The table builds the
`ai.veoveo/authorization` declaration through `AuthorizationBuilder`, which registration
sends and MCP Discover advertises. Every handler resolves its own target through that
declaration and checks the result against the permissions in the forwarded access
token, so the Gateway and Time evaluate the same table. A target the declaration omits
is denied. Task admission checks the tool's permission before it creates a Task.
Declared templates use simple variables only; `time://zones/{zone_id}` matches zone IDs
that contain slashes, and `?cursor={cursor}` templates admit later collection pages.

Native tests check that every listed tool has exactly one requirement, that every
advertised resource and concrete address resolves, that tool annotations agree with
the declaration, and that each handler admits its permission and refuses the others
through the hosted router.

The forwarded token also carries the caller's principal, tenant, roles, data labels
and invocation authority. Task and event ownership is derived from that identity.

## Registration

Time registers with the Gateway at startup, before it serves traffic:

1. `KeyDirectory` loads Time's Ed25519 key from `VEOVEO_KEY_DIRECTORY`, generating it on
   first start. The key keeps Time's registration identity across restarts.
2. `RegistrationClient::discover` reads the registration endpoint from the RFC 8414
   metadata of `VEOVEO_GATEWAY_ISSUER`.
3. Time registers its slug, URI scheme, version, MCP endpoint
   `{VEOVEO_PUBLIC_BASE_URL}/time/mcp`, authorization declaration and `time` module
   manifest. The installation's registration secret, `VEOVEO_REGISTRATION_SECRET`, is
   the initial access token.
4. The response supplies the issuer and keys that verify forwarded access tokens and
   the SurrealDB endpoint, namespace, database and user Time connects with.
5. A background task renews the registration at the interval the lease names and
   removes it when the server shuts down.

A registration without a database grant stops startup.

## Deployment

The image runs as UID 10001 and can use a read-only root filesystem. Writable paths
are the Time persistent volume and `/tmp`. The container includes CA roots, `tzdata`,
`zic`, and the `veoveo-mcp-time` binary.

An installation runs one replica with a `ReadWriteOnce` volume because release
activation and local event watchers are process-owned. The Gateway reaches the service
only over the installation network. SurrealDB holds the durable coordination state.

Connected installations grant the pod HTTPS egress only to approved IANA mirrors or
installation-controlled authority endpoints. Source endpoints remain tenant admin
records. Offline installations use the image bootstrap authority and can stage
content through an approved internal HTTPS endpoint.

The optional ntpd-rs observation socket is supplied with
`--ntpd-observation-socket` or `VEOVEO_NTPD_OBSERVATION_SOCKET`. A node deployment mounts that Unix socket and grants the
Time process read/connect access according to the installation's clock architecture.

## Cross-Domain Use

Map requests can carry canonical `TimeInstant` values for departure time, restriction
validity, traffic snapshots, weather windows, tides, and authority-effective routing.
Optimization can expand calendars and validate timeline constraints before assigning
vehicles, crews, facilities, and route legs. Frames can use mission-relative epochs for
sensor and platform transformations while keeping Earth geography in Map.

Examples of agent requests include:

- Resolve `141530ZJUL26`, then convert it to the warehouse and destination zones.
- Assess whether this node meets a 5 ms error budget with two independent sources.
- Expand calendar version 4 across the convoy planning horizon through the Task API.
- Validate that border clearance follows arrival by 15 to 45 minutes.
- Create an event at H-hour plus 90 seconds and subscribe to its resource.
- Intersect crew availability, port access, and daylight windows.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/contract/` | strong ids, time expressions, calendars, events, admin models |
| `src/authority.rs` | TZif context and IANA leap-second interpretation |
| `src/engine.rs` | resolution, projection, recurrence, timelines and engine authority checks |
| `src/engine/windows.rs` | interval-set algebra with input endpoint metadata |
| `src/clock.rs` | observation adapter and clock-policy assessment |
| `src/contract/clock_policy.rs`, `src/contract/version.rs` | validated policy builder, numeric request schemas, positive versions and optional-row guards |
| `src/contract/permissions.rs` | `TimePermission` names, resources, access and risks |
| `src/catalog/clock.rs` | checked stored clock-policy scalars, identity and version |
| `src/contract/instant.rs` | checked subsecond values, instant metadata and lossless total-coordinate conversion |
| `src/contract/authority.rs` | checked release references, effective authority pairs, derived bindings and wire adapters |
| `src/contract/resolution.rs` | resolved instant/release agreement, read-only metadata and flat projection wire fields |
| `src/contract/window.rs` | checked half-open bounds, authority agreement and metadata-preserving intersection |
| `src/catalog.rs`, `src/catalog/pages.rs` | typed catalog operations, domain body decoding, collection envelopes and completion |
| `src/catalog/records.rs` | current body/key and indexed-field checks, lifecycle-column decoding and redacted metadata errors |
| `src/catalog/knowledge.rs` and `src/persistence/provenance.rs` | source-owned knowledge reads, stored creation context and checked observation attribution |
| `src/persistence/` | private typed query/mutation interfaces, SurrealDB driver records, admission and SQL visibility |
| `src/persistence/active.rs`, `src/persistence/activation.rs` | joined pointer/release admission and transactional activation relationship checks |
| `src/index.rs` | collection-bound opaque cursors and page envelopes |
| `src/registry.rs` | request-validated tenant contexts, isolated engine state and preflight-bound activation |
| `src/catalog/activation.rs` | private observed activation drafts and their publication |
| `src/acquisition/` | bounded download, validation, compilation, staging, cancellation |
| `src/admin/` | typed administrative routes, the administrative permission check and problem details |
| `src/mcp.rs` | MCP tools, resources, templates, completions, subscriptions |
| `src/mcp/requirements.rs` | the requirement table, the authorization declaration and handler checks |
| `src/mcp/declarations.rs` | registration identity and the `time` module declaration |
| `src/prompts.rs` | reusable temporal interaction prompts |
| `src/server/tasks.rs` | final Task API adapter, leases, recovery, subscriptions |
| `src/server/` | configuration, HTTP assembly and startup |
| `src/server/registration.rs` | key loading, Gateway discovery and registration |
| `src/schema/migrations/0000_current.surql` | Time-owned temporal schema and indexes |
| `src/persistence/queries/`, `src/tests/queries/` | production statements and native fixture scripts |

## Verification

Read-only public-gateway acceptance covers the selected
authority pair, clock, resolution and conversion vectors, versioned calendar and epoch,
epoch arithmetic, window intersection and HTTPS authority-source metadata. It runs
through the Gateway from this repository's environment, and its inputs follow the
[installed harness contract](https://github.com/BiomaAI/veoveo/blob/0c33cf8a7e5a1d46f953702553ac6643f99c1363/testing/installed/DESIGN.md#time-consumers).
Native Task controls use two worker identities on separate connections to the same
isolated Store. They commit cancellation between completion snapshot selection and
settlement, verify remote and local cancellation, preserve completion and calculation
failure outcomes, and reject unrelated conflicts or another worker's live lease.
A local-token control checks that shutdown without durable cancel intent leaves the
original Task unfinished without a result. Selected-snapshot controls stop success
publication after owner reads and exercise the reconciliation dispatch seam against
actual retained Task state.
These controls do not qualify installed cancellation or unfinished process restart.

Local controls qualify fixture and receipt handling, clock classification, touching
and one-nanosecond interval assertions, and incomplete HTTP response tracing. Installed execution,
Task delivery, restart recovery, authority activation and conflict rollback require
their separate qualification.

Unit tests cover leap authority validation, positive-leap projection, RFC/GPS/DTG
equivalence, DST ambiguity, DST-aware schedule expansion, half-open interval algebra,
timeline violations, clock policy, canonical URIs, acquisition configuration, and
archive traversal rejection.
Time runtime tests cover stored URL/ID admission, SQL isolation and pagination, requested
epoch batches and completion. Retained-metadata cases corrupt rows through a separate
fixture connection, verify read/collection rejection, preserve lifecycle columns, and
reject acquisition identity changes before writing. Isolated authority tests use separate connections to
qualify retirement, competing pointer updates and exhaustion rollback followed by retry.
Numeric contract cases compare schema bounds with JSON admission, and compile-fail
examples reject unchecked construction. Native scalar cases reject negative, zero,
truncated and exhausted values without changing rows; competing clock replacements
admit one writer. Window contract cases check JSON admission, read-only bounds and
signed-coordinate extremes. Native interval cases compare all three operations with
half-open membership and check endpoint uncertainty; schedule cases cover horizon
clipping, recurrence limits and foreign authority rejection.
Resolution contract cases check flat wire fields, schema admission and release agreement
inside both output models. Native epoch cases cover offset fractions, authority rejection
and uncertainty preservation, including overflow without epoch mutation.
Active-pointer cases corrupt family, identity, version, history and
release links; raw query checks prove SQL excludes denied release payloads. Fixture
events change pointer and previous-release fields after the candidate update, proving
that the production transaction rechecks them and rolls back all changes. Time fixtures
select the Time module lane explicitly alongside the required kernel lanes. Registry cases use copied Linux UTC tzdata and temporary leap
files. They qualify persisted selection on a fresh replica, request-time refresh without
notifications, independent epoch maps, cache reuse, failed-load eviction and recovery,
native LIVE/reconciliation eviction through separate database connections, and batched
watcher admission that skips existing and terminal events.
Permission cases issue access tokens with chosen grants and call every permission
family through the hosted router. The container build and the shared SurrealDB fixture
exercise the deployment boundary.

## Replica Resource Observation

Each replica opens one shared group of projected Store LIVE queries for authority releases and activation, acquisitions, calendars, epochs, events and clock policy.
Committed changes invalidate only each listener's accepted resource identities and
requested catalog. Writes coalesce over 100 milliseconds. Source reconnection invalidates readers after a delivery gap; reads retain normal
current authority. Idle sources emit no periodic resource-change notifications. This observes durable state and cannot dispatch work.

## Identity Declaration Mechanics

Public Time ID declarations use `Id` with Time-owned prefix and lexical admission. String conversion preserves the supplied spelling and the existing schema profile. Temporal numbers, instants, release relationships and cursors keep their own validation contracts.

## Value Admission

Time windows, authority bindings, effective authority pairs and resolution outputs retain their Wire fields through immutable `Checked` storage. Owner checks enforce ordered coordinates and release/dataset agreement. TimeAuthorityReference keeps the explicit redundant release-ID adapter; the temporal engine owns projection calculations.

## Cursor Admission

The five temporal cursors select distinct collection profiles through one generic
owner codec. Each profile binds its position type and collection URI at compile time;
acquired and bootstrap authority releases have separate profiles even though both use
`AuthorityReleaseId`. `OpaqueCursor` retains admitted original text, including
hexadecimal-case and JSON spelling aliases. Transparent Serde delegates admission to
the stateless codec. Each public wrapper keeps its own string schema identity.
Constructors keep their infallible signatures and parser size policy; copied and
borrowed position accessors preserve the owner API.

## Persistence Module Declaration

The independent `schema` feature exports `schema::module_setup(execution)` for the
`time` optional module. It activates `veoveo-modules` with default features disabled and the foundational
vocabulary, Serde and schema dependencies used by owner table declarations. MCP,
Store and asynchronous runtime dependencies require their own features. The
declaration claims `time_*`.
It requires Identity and its Store requirement.

The lane embeds `src/schema/migrations/0000_current.surql`, which defines the
Time tables and their indexes. The composition root supplies the checked execution
image and command. Installation execution and its Jobs require separate qualification;
the runtime server never applies the lane. Owner migrations and queries live in this
crate, with one declaration per schema object.

Artifact path text and client-facing temporal relationships do not invent optional schema requirements.

## Persistence Observation

`TimeObservationTable` declares the owner's closed observation table names under the
`schema` feature. Runtime consumers convert these declarations into checked
`ObservationTable` descriptors and compose them with kernel tables. The descriptor
admits an identifier; it does not certify installed schema or grant read authority.
These owner tables declare 30-day changefeed retention matching the installed SQL.
LIVE invalidation and changefeed recovery keep their existing reconciliation and
checkpoint behavior. Public DTO contract features do not activate observation sources.

## Task Completion Products

Schedule expansion and timeline validation return inline calculations with their
existing typed temporal provenance. These Task completions create no result resource
and omit `result_uri`; MCP envelope admission records that distinction.
