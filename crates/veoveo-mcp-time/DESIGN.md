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
| [Veoveo knowledge source extension](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/ext-knowledge/DESIGN.md) | `ai.veoveo/knowledge-source` declares documentation and Time-owned collections, typed source observations, conditional reads, and enumeration from each collection root. Resource updates follow database change notifications. |
| [Veoveo server registration](https://github.com/VeoveoAI/veoveo-core/blob/37eab1febc73d116a6dc6e8a285604c5be551a11/mcp/ext-registration/DESIGN.md) | Time registers with the Gateway through OAuth 2.0 Dynamic Client Registration (RFC 7591) and removes the registration through RFC 7592 when it stops. It discovers the registration endpoint from RFC 8414 metadata and advertises `ai.veoveo/registration` and `ai.veoveo/authorization` in MCP Discover. |
| [JWT access tokens RFC 9068](https://www.rfc-editor.org/rfc/rfc9068.html) | The Gateway forwards each admitted request with an EdDSA-signed access token whose RFC 9396 `authorization_details` carry the caller's granted Time permissions. Time verifies the token against the issuer and keys its registration returned. |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | UTC and numeric-offset timestamp input and canonical UTC output, including explicit leap-second handling. |
| [RFC 9557](https://www.rfc-editor.org/rfc/rfc9557.html) | Timestamp input with an IANA time-zone annotation and explicit ambiguity policy. |
| IANA Time Zone Database and [TZif RFC 8536](https://www.rfc-editor.org/rfc/rfc8536.html) | Versioned civil-time authority, compiled release products, zone completion, and fold/gap resolution. |
| IANA `leap-seconds.list` | Versioned TAI-UTC transition authority. Every canonical instant binds its TZDB and leap-second releases. |
| SHA-256 digests | Requests, stored metadata and provenance use the foundational `Sha256Digest` spelling, `sha256:` followed by 64 lowercase hexadecimal digits. |
| TAI, UTC, TT, TDB, GPS, and Galileo system time | Typed projections from one integral TAI instant. The server records authority and uncertainty rather than treating scales as interchangeable strings. |
| [NTPv4 RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html) and [Network Time Security RFC 8915](https://www.rfc-editor.org/rfc/rfc8915.html) | Approved node clocks may use NTP/NTS. Time MCP consumes a bounded `ntpd-rs` observation; it does not act as an NTP network endpoint. |
| HTTPS | Registered IANA authority sources are acquired under fixed host, media, digest, size, and elapsed-time policy. |
| [PostgreSQL 18](https://www.postgresql.org/docs/18/) | Server 18.6 through [sqlx](https://docs.rs/sqlx/0.9.0) 0.9.0, as the [installation database design](https://github.com/VeoveoAI/veoveo-core/blob/7632308/docs/DATABASE.md) describes. Time owns the schema `time`, applies its own migrations from `migrations/` and runs every statement from a `.sql` file that sqlx checks against the migrated schema. Row triggers publish changes on the `time` channel through `LISTEN`/`NOTIFY`; activation serializes on a transaction advisory lock and `SELECT … FOR UPDATE`. |

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

Administration tools report a missing record, a version conflict or an invalid value
as a tool execution error in the result, as MCP specifies, and anything else as an
internal error.

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

A claimed Task whose persisted request no longer decodes settles as failed.

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
cursor type; resource reads and event recovery use those same types. The
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
  |-- IANA acquisition and zic compilation
  |-- ntpd-rs observation adapter
  |-- PostgreSQL schema `time`, change listener and due loop
  `-- persistent authority release volume
```

The Rust process owns the HTTP server, acquisition jobs, active authority cache, the
change listener, the event due loop and Task API workers. The same image contains
`zic`, the bootstrap TZif database, and the bootstrap IANA leap-second file supplied by
Debian `tzdata`.

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

One PostgreSQL transaction marks the candidate active, advances the family pointer
and retires the superseded release. The candidate update requires its tenant, key,
family, staged state and expected version. Pointer replacement rechecks its tenant,
family, current release, previous-release history and version. Retirement requires
the previous release's tenant, family, key, active state and observed version below
the storage maximum. A statement that matches no row rolls back the candidate, pointer
and retirement together and reports a typed `ActivationConflict` naming the failed
check. The process then replaces the tenant engine; the pointer's row trigger notifies
subscribers of `time://authorities/current` on every replica.

Authority release records retain source id, source URL, SHA-256 digest, retrieval
time, validation time, version label, artifact path, lifecycle state, and optimistic
record version.

`catalog/activation.rs` prepares a private draft containing the staged candidate
and the complete admitted pointer/release snapshot. The registry loads the prospective
pair from that draft under its 30-second deadline, then consumes the draft to publish.
The public runtime activation method always performs file preflight.
The commit compares both families and the candidate with their observed metadata;
an absent pointer is part of that comparison. Changed metadata rejects publication
even when an external repair did not advance the version.

The transaction first takes a transaction advisory lock keyed by the tenant, so the
activations of one tenant run one at a time. Row locks cannot cover a pointer that
does not exist yet, and the advisory lock does. The transaction then locks the
tenant's pointers and the candidate and preflight releases with `SELECT … FOR UPDATE`
in key order, rereads the pointer/release snapshot and the candidate, and compares
them with the preflight observation before changing a row. A writer that repairs a
locked row waits until the activation ends. Clients load the current pair again
before retrying; the server performs no automatic mutation retry.

The schema holds the relationships that need no transaction. A pointer references a
release of its own tenant and family through a composite foreign key, as does its
previous release. Its history agrees with its version: version one has no previous
release, and a later version names one other than its current release. Releases and
acquisitions reference their tenant's source, and an acquisition's staged release must
exist. Native qualification covers different-family contention starting with absent
pointers, existing pairs, the locks held against concurrent repairs, stale preflight
metadata, relationship changes inside the transaction, tenant mismatch and failed file
loads.

### Tenant Authority Contexts

Every engine request reads the active tenant pair and resolves each selected release's
acquisition provenance. The pointers and their releases come from one repeatable-read
snapshot, followed by at most two provenance reads; authority-only requests do not
load epochs. A family without a stored pointer uses its packaged bootstrap reference.
Invalid visible metadata and database errors fail the request.

The registry caches loaded contexts under the Store's typed tenant identity. A hit
requires the current effective references, provenance and artifact paths to match
the cached inputs. An engine gets its own epoch map while sharing the loaded
authority data. A change notification for a tenant's release, acquisition or pointer
evicts that tenant's context, and a listener reconnect evicts every context; a
generation token prevents an earlier loader from repopulating an evicted cache.
Request-time catalog checks also cover delayed or disconnected notification delivery.

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
named by its input expressions. One `DISTINCT ON` query selects the latest version of
each key inside the caller's tenant, in batches of at most 100 keys. The request has a
30-second epoch-read deadline and accepts at most 100,000 expressions. A calculation's
epoch set belongs to that calculation, so concurrent requests cannot replace it.
Engine maps keep `EpochId` keys. Relative resolution requires the epoch's
authority pair to match the active engine and preserves its uncertainty while adding
the requested offset. Additional resolution uncertainty uses checked addition; a sum
outside the unsigned 64-bit nanosecond range fails. Activation may therefore require
publishing a new epoch version under the selected authority before relative calculations
can resume. Exact reads keep the stored epoch's original binding.
An exact epoch resource read selects the latest version of its tenant and epoch key
through the same query. Clock observations, exact resource reads,
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

Tasks persist through `veoveo-task-runtime` in the kernel `tasks` schema. Ownership
includes tenant, principal, profile, server, and data labels. Creating a Task queues
it. Each process consumes the runtime's claim stream: a claim records the process
instance as the Task's owner, and the process decodes the persisted request and runs
it. A request that does not decode as a Time calculation settles as failed. Both
temporal task types use `Resume` recovery because their outputs are deterministic under
the persisted authority-bound request. When a process dies, the kernel reaps its
instance and returns its Tasks to the queue, where any replica claims and resumes
them; no lease or heartbeat is involved. A lost instance lock ends the claim stream,
and Time then stops serving. Terminal task records retain for seven days unless a
retention pin extends their lifetime.

Workers read durable cancellation before scope resolution, after scope resolution,
and after authority and epoch loading. These checkpoints observe requests committed
through another server replica without relying on a shared local cancellation token.
Hosted schedule expansion and timeline validation run finite calculations on Tokio's
blocking pool. The registered async worker retains the original job outside its
cancellable waiter and awaits that job before it exits. Cooperative stop checks occur
at calendar windows, recurrence days, timeline points and constraints, and output
indexing. Public engine methods stay synchronous for library callers. Sorting and
serialization finish between checkpoints; this profile promises neither hard abort
nor a fixed drain latency. Hard process termination follows instance recovery.
A local shutdown stop without durable cancellation prevents result publication and
leaves the request for `Resume` recovery once the instance is reaped; it does not
fabricate a terminal cancellation. Success publication carries that token through snapshot
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
when cancellation precedes its settlement. The worker checks that its instance owns
the Task before attempting a write; every worker write also compares the owner
instance and revision in SQL, so a process whose instance was reaped settles nothing.
Unrelated progress conflicts and storage errors stop execution without being reported
as successful settlement. This path performs no polling.

The installed schedule profile, kept in the monorepo's
[installed harness](https://github.com/BiomaAI/veoveo/blob/0c33cf8a7e5a1d46f953702553ac6643f99c1363/servers/time-mcp/tests/gateway_consumers/schedule.rs), selects
completion, cancellation or unfinished process recovery explicitly. Cancellation
and recovery require both delivered and current Working state for the acknowledged
opaque Gateway Task identity. Recovery admits one maintained Rust container, observes
its externally signalled exit and same-Pod replacement, then requires the same Task
still Working before accepting delivered Completed and its independently expected
result. A completed Task surviving replacement does not qualify unfinished recovery.

The fixture sets one operation deadline before connecting: 120 seconds for completion
or cancellation and 300 seconds for recovery. Recovery needs room for the kernel to
reap the dead instance before another worker resumes computation. Every nested wait
ends by that operation deadline. The scenario grants
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
modification time, and event changes reach subscribers through the `time` channel.
Events do not claim a last-modifying principal because due transitions record no
actor. Immutable members attribute creation to their stored owner. Member JSON is
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
`cursor` query parameter, as in `time://events?cursor=<nextCursor>`. SQL applies
the tenant and owner predicates and the cursor position as a keyset condition before
its 101-record lookahead, and indexes cover each order. Each collection has one
statement for its first page and one for the pages after a cursor, so no predicate tests
whether a cursor is present and PostgreSQL's cached generic plan seeks the index past the
position. Calendars and epochs sort by key ascending and version descending, a mixed
order that no row comparison expresses: the next-page statement bounds the key with
`>=` the cursor's key, which the page index seeks, and filters the versions of that key
already returned. Events sort by
TAI seconds, nanosecond, then event key ascending. These positions use immutable
fields. Each page observes the records present at its read. Inserts before its position
appear on a fresh walk.

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
predicates, prefix matching, distinct selection, and ordering in SQL before fetching at
most 101 candidates. MCP returns 100 values and sets `hasMore` when a
further candidate exists. It reports a total only when the complete match set fits.
Stored IDs are lowercase printable ASCII, so an ID prefix matches case-insensitively and
becomes the half-open key range `[prefix, successor)` that the `COLLATE "C"` indexes
seek. Every character compares literally, and a prefix with any other character matches
nothing. Version completion matches the leading digits of each version number.
Calendar-version and epoch-version completion narrow their SQL query to the selected
`calendar_id` or `epoch_id` when the client supplies that argument in completion
context.
Calendars, epochs, authorities, the clock policy and events emit resource updates.
Every row change of a Time table notifies the `time` channel with the table, the
operation, the row key and the tenant. Each replica's change watcher maps the table
to the subscribable resources it can alter and tells only that tenant's listeners:

| Table | Resources updated |
|---|---|
| `time.event` | `time://events` and `time://events/{event_id}` |
| `time.epoch` | `time://epochs` and `time://epochs/{epoch_id}` |
| `time.calendar_version` | `time://calendars` |
| `time.active_authority` | `time://authorities/current` and `time://clock/current` |
| `time.clock_policy` | `time://clock/current` |

A `subscriptions/listen` request listens to the hub of its caller's tenant, which the
watcher keeps while a listener holds it. Notifications are wake signals: a subscriber
rereads the resource under its own authorization. When the listener connection
returns after a loss, the watcher asks every listener to reread each resource it
watches, because PostgreSQL does not replay notifications sent while it was
disconnected.

### Prompts

| Prompt | Purpose |
|---|---|
| `resolve_operational_time` | guide authority-aware normalization |
| `expand_operational_calendar` | prepare a bounded Task API expansion |
| `validate_mission_timeline` | prepare named points and separation constraints |

## Events

Events are owner-scoped durable records. Creation validates an authority-bound due
instant and uses a principal-scoped idempotency key. The row stores the instant's TAI
coordinates and `due`, the earliest UTC microsecond that does not precede it under the
active authority; an instant inside a positive leap second becomes due at the start of
the next second. Cancellation updates the record under optimistic concurrency.

Each process runs one due loop. It reads the earliest `due` of a scheduled event
together with the database clock, sleeps for the difference or until an event change
or a listener reconnect wakes it, and then runs one statement that moves every
scheduled event whose `due` has passed to `due`, advancing its version and body.
Replicas run the same statement, and row locks with the `scheduled` predicate change
each event once. An event that became due while no process ran changes at the next
start. The row trigger then notifies subscribers on every replica. An event whose
version is exhausted stays scheduled rather than failing the statement for others.

The installed future-event consumer arms the stable event-root subscription before
creation and reads the current authority-bound clock to construct checked TAI
coordinates. Delivered invalidations require matching uncached event reads;
Scheduled and Due are separate assertions. A deliberately disconnected interval
qualifies current-state reconciliation rather than notification replay.
The profile owns one 300-second operation and 30-second cleanup interval, retains
ambiguous creation intents, and cancels only acknowledged owned Scheduled events.
Due and cancelled event records remain for operator retirement. Agent wake scheduling uses
these public resources through the agent's own owner identity and policy.

Event reads, collections and optimistic transitions bind both the tenant and
authenticated owner in SQL. Completion applies the same owner predicate before its
limit. The catalog decodes only the records selected by those queries.

## Administration Tools

Tenant administration uses MCP tools over the same typed temporal models, catalog and
task state as the domain tools. Every tool requires a permission that carries the
`administer` risk, so the Gateway grants it only to roles that allow that risk.

| Tool | Permission | Operation |
|---|---|---|
| `list_sources`, `get_source` | `time.authorities.inspect` | read registered authority sources |
| `create_source` | `time.authorities.administer` | register a source, idempotent by key |
| `replace_source` | `time.authorities.administer` | replace a source under optimistic concurrency |
| `list_acquisitions`, `get_acquisition` | `time.authorities.inspect` | read acquisition progress |
| `start_acquisition` | `time.authorities.administer` | start an idempotent acquisition |
| `cancel_acquisition` | `time.authorities.administer` | request cancellation |
| `list_active_authorities` | `time.authorities.inspect` | read active family pointers |
| `activate_release` | `time.authorities.administer` | preflight and activate a staged release |
| `create_calendar` | `time.calendars.write` | publish an immutable calendar version |
| `create_epoch` | `time.epochs.write` | publish an epoch version |
| `replace_clock_policy` | `time.clock-policies.write` | replace the clock policy under optimistic concurrency |

Releases, calendars, epochs and the clock policy are read through Time's resources.
An acquisition is a durable job with its own idempotency, phases and cancellation:
`start_acquisition` returns its record, and `get_acquisition` reports its progress.

`list_sources` and `list_acquisitions` return keyset pages, `AdminPage<TimeSource,
SourceCursor>` and `AdminPage<TimeAcquisition, AcquisitionCursor>`. A request names an
optional `cursor` and a `limit` of 1 to 100 records, 100 when omitted. SQL reads one
record past the limit, and `nextCursor` appears only when that record exists. Sources
list in source ID order. Acquisitions list newest first: Time assigns their UUIDv7 IDs,
so ID order is the order they started. Both orders seek the table's primary key. Each
cursor is an opaque envelope that names its listing, so neither listing nor a resource
collection admits another's cursor. The tenant predicate applies to every page.

`list_active_authorities` returns `AdminPage<ActiveAuthoritySelection, NoCursor>`, one
page with at most one selection per family. `NoCursor` has no value, so `nextCursor` is
always null. Each closed
entry contains `pointerVersion` and `release`, an admitted active `AuthorityRelease`.
The positive pointer version advances independently of the release's `recordVersion`.
An operator passes the selection's `write_guard()` as
`ActivateReleaseRequest.expected_active_pointer_version`; the method constructs
`TimeWriteGuard::Existing(pointerVersion)` rather than using the release version.

The catalog reads pointers and releases from one snapshot and admits their tenant,
key, family, active state and stored body relationships. A visible invalid
pointer fails the read. An omitted family has no persisted pointer and uses
`TimeWriteGuard::Absent` for its first activation; an empty page means neither
family has a persisted pointer. Bootstrap effective references belong to
`time://authorities/current` and do not invent persisted pointer versions.

## Persistence

Time keeps its catalog in the PostgreSQL schema `time` of the installation database.
The Gateway creates the schema, owned by the group role `server_time`, when Time
registers. At startup Time takes that role on a dedicated connection and applies the
migrations in `migrations/`, so the group role owns every object and the history lives
in `time._sqlx_migrations`. Concurrent replicas wait on the migrator's advisory lock,
and a changed checksum of an applied migration stops startup.

The runtime's private `persistence/` modules own the queries, driver records,
mutation drafts and validation. `PlatformStore` supplies the pool and platform
identity; the connection logs in as the registration's own login. Every statement is a
file in `src/persistence/queries/`, run through `query_file_as!`, `query_file!` or
`query_file_scalar!`, which check it against the migrated schema at compile time.
Builds without a database use the workspace's committed `.sqlx` metadata, which
`cargo sqlx prepare --workspace` regenerates against a database with the kernel
migrations and Time's migrations applied. Native fixture statements live in `src/tests/queries/`.

Catalog calls retain source, release, acquisition, calendar, epoch and event ID types
through the persistence interface and into the driver: the IDs, the persistence
vocabularies and `TimeAcquisitionPhase` bind as `text` through their `postgres` hooks,
which the `runtime` feature enables, and tenants and principals bind as `uuid`.
Contract-only consumers resolve no database dependency. Calendar, epoch and
existing-record guards use `TimeVersion`. Collection queries accept `CalendarCursor`,
`EpochCursor`, `EventCursor`, `AuthorityCursor`, `SourceCursor` or `AcquisitionCursor`
directly. Completion selects one of eight fixed queries through `TimeCompletion`, and
an ID prefix reaches SQL as its typed key range. Tenant and event-owner predicates,
keyset positions, sorting, distinct selection and limits execute in SQL.

Public Time IDs accept bounded prefixed names because bootstrap authority provenance
can name image-provided releases. Stored keys are the prefix followed by a lowercase
hyphenated UUIDv7; `validate_key` and the schema's `time.is_key` check admit the same
form. Public syntax does not establish that a stored record exists. Bootstrap
references keep their public spelling and are not converted into database keys. The
Time contract defines JSON bodies and cursor versions.

| Table | Key | Responsibility |
|---|---|---|
| `time.source` | tenant, source key | registered IANA source endpoints and media policy |
| `time.authority_release` | tenant, release key | immutable release provenance and lifecycle |
| `time.active_authority` | tenant, dataset kind | one optimistic pointer per tenant and family |
| `time.acquisition` | tenant, acquisition key | idempotent acquisition state and progress |
| `time.calendar_version` | tenant, calendar key, version | immutable operational calendars |
| `time.epoch` | tenant, epoch key, version | versioned named physical instants |
| `time.event` | tenant, event key | owner-scoped scheduled events |
| `time.clock_policy` | tenant | tenant clock acceptance policy |

Tenants and owners are `uuid` columns without foreign keys into the identity schema.
Vocabularies are `text` with a `CHECK` listing their spellings, and counters are
`bigint` with a `CHECK` for their range. Each row's `body` is a `jsonb` object holding
the public document. Unique constraints admit one acquisition and one event per tenant,
owner and idempotency key, and one release per source and digest. Records that keep
their creator's provenance store the tenant key, owner key and Work Context beside
their owner. Every table carries the `kernel.notify_change` row trigger.

Time records every timestamp at microsecond precision, the precision of `timestamptz`,
so a JSON body and its columns name the same instant on every platform clock.
`admission::stored_timestamp` drops the nanoseconds below the microsecond.
`AuthorityReleaseValue::build` applies it to retrieval and validation times, and the
catalog applies it to acquisition times before serializing the body.
`admission::stored_deadline` rounds an event's due instant up to the next microsecond,
so the due loop never fires an event early. Release and acquisition decoding rejects a
finer timestamp, and persistence validation rejects one bound to a column. The database
clock sets `created_at`, `updated_at` and `activated_at`.

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
inside that transaction with a below-maximum version predicate; failure rolls back the
candidate release and pointer together. The schema keeps every stored version
positive.

### Acquisition Phases

`TimeAcquisitionPhase` owns queued, downloading, validating, complete, cancelling,
cancelled and failed spellings across the public contract, worker, driver and schema.
The database rejects unknown phases before changing a row; contract-only consumers
use the same vocabulary without database dependencies. Status and phase keep their
separate transition responsibilities.

### Retained Catalog Metadata

`catalog/records.rs` converts SQL-selected rows into public values. It checks each
stored key against the typed ID in the JSON body. The schema keeps calendar and epoch
versions positive and event and epoch nanoseconds below one billion. SQL applies
tenant and event-owner visibility before this conversion; a visible inconsistent record
fails its read or page without being dropped from the results. Reads never repair or
rewrite retained data.

| Entity | Body fields that must agree with the row | Values taken from lifecycle columns |
|---|---|---|
| Source | ID, name, dataset kind, URL, content type, enabled flag | record version |
| Authority release | release and source IDs, dataset kind, version label, URL, digest, artifact path, retrieval and validation times | state, record version |
| Acquisition | acquisition and source IDs, expected digest | status, phase, staged release ID, record version, creation and update times |
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

`catalog/clock.rs` checks the clock policy's tenant, converts stored signed scalars
without narrowing, and admits the policy and version through their contract types.
The schema's checks hold the same ranges as the policy builder. Invalid scalar
diagnostics name the field without quoting stored values.

Every stored instant fraction must be within `0..=999999999`. Each instant binding
and packaged bootstrap pair must use distinct release IDs for TZDB and leap seconds.
Reference decoding checks repeated URI/ID agreement, source location, dataset roles
and nonblank labels. The public instant and Unix/TAI schemas declare the same
subsecond range. Zero-version update requests apply only to an absence guard.

Time decodes only the current body format and supplies no historical-body adapter.
Changing a stored body format or a bootstrap identity starts from a fresh schema. Epochs
and events that reference a withdrawn packaged authority are recreated rather than
reinterpreted under a new release. Acquired authority releases keep their stored
identities and artifacts. Bootstrap IDs are derived by the server and have no
configuration override or alias.

`persistence/active.rs` reads tenant pointers and the releases they select in one
repeatable-read transaction. The release query applies tenant, family, key and
active-state predicates. A visible pointer without a matching release fails the read;
only an absent pointer permits bootstrap selection during reload. Pointer admission
checks its tenant, positive version, stored release-ID syntax and previous-release
history. Catalog decoding then checks the selected release body. These checks leave
stored records unchanged and apply after every restart.

Compiled authority products live under `/var/lib/veoveo/time/releases`. Acquisition
scratch data lives under `/var/lib/veoveo/time/acquisitions` and is removed at terminal
completion. Time's registration key lives under `/var/lib/veoveo/time/keys`. A
deployment mounts `/var/lib/veoveo/time` from a persistent volume.

## Authorization

The Gateway grants permissions, and Time checks every request against them again.
`TimePermission` names nine permissions in Time's namespace:

| Permission | Resource | Access | Grants |
|---|---|---|---|
| `time.authorities.read` | `time.authority` | read | `resolve_time`, `convert_time`, `assess_clock`, every resource and template, prompts, resource listing and completions |
| `time.schedules.expand` | `time.schedule` | read | `evaluate_windows` and the `expand_schedule` Task |
| `time.timelines.validate` | `time.timeline` | read | the `validate_timeline` Task |
| `time.events.write` | `time.event` | write | `create_event` and `cancel_event` |
| `time.authorities.inspect` | `time.authority` | read, administer risk | `list_sources`, `get_source`, `list_acquisitions`, `get_acquisition`, `list_active_authorities` |
| `time.authorities.administer` | `time.authority` | write, administer risk | `create_source`, `replace_source`, `start_acquisition`, `cancel_acquisition`, `activate_release` |
| `time.calendars.write` | `time.calendar` | write, administer risk | `create_calendar` |
| `time.epochs.write` | `time.epoch` | write, administer risk | `create_epoch` |
| `time.clock-policies.write` | `time.clock-policy` | write, administer risk | `replace_clock_policy` |

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
   `{VEOVEO_PUBLIC_BASE_URL}/time/mcp` and authorization declaration. The
   installation's registration secret, `VEOVEO_REGISTRATION_SECRET`, is the initial
   access token.
4. The response supplies the issuer and keys that verify forwarded access tokens and
   the `veoveo_database` grant: the database URL and the login and password the
   Gateway created for this registration, a member of `server_time`.
5. Time connects as that login, applies its migrations, starts its process instance,
   its change listener and its due loop, and serves.
6. On SIGTERM or Ctrl-C Time stops serving and stops its instance.
   `RegistrationClient::maintain` then deletes the registration through RFC 7592, which
   drops the login.

The registration lasts while Time's login holds a live process instance. When Time
exits without deleting it, the Gateway removes it after the installation's grace period.
Time sends no updates or renewals. A registration without a database grant stops
startup, and Time still deletes it.

## Deployment

The image runs as UID 10001 and can use a read-only root filesystem. Writable paths
are the Time persistent volume and `/tmp`. The container includes CA roots, `tzdata`,
`zic`, and the `veoveo-mcp-time` binary.

An installation runs one replica with a `ReadWriteOnce` volume because acquisition
artifacts live on that volume. Activation, due transitions and change notifications
coordinate through PostgreSQL, which holds the durable state. Time connects to
PostgreSQL directly, without a pooler that multiplexes sessions, because its instance
lock and change listener hold session state. The Gateway reaches the service only over
the installation network.

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
| `src/persistence/` | private typed query/mutation interfaces, driver records, admission and SQL visibility |
| `src/persistence/active.rs`, `src/persistence/activation.rs` | pointer/release snapshot admission and the locked activation transaction |
| `src/persistence/due.rs`, `src/due.rs` | the due statement and the per-process due loop |
| `src/changes.rs` | Time's schema and channel, the change watcher and tenant subscription hubs |
| `src/mcp/subscriptions.rs` | tenant-scoped `subscriptions/listen` admission |
| `src/index.rs` | collection-bound opaque cursors and page envelopes |
| `src/registry.rs` | request-validated tenant contexts, isolated engine state and preflight-bound activation |
| `src/catalog/activation.rs` | private observed activation drafts and their publication |
| `src/acquisition/` | bounded download, validation, compilation, staging, cancellation |
| `src/mcp.rs` | MCP tools, resources, templates, completions, subscriptions |
| `src/mcp/administration.rs` | tenant administration tools |
| `src/mcp/requirements.rs` | the requirement table, the authorization declaration and handler checks |
| `src/mcp/declarations.rs` | registration identity |
| `src/prompts.rs` | reusable temporal interaction prompts |
| `src/server/tasks.rs` | final Task API adapter, the claim-stream consumer and settlement |
| `src/server/` | configuration, HTTP assembly and startup |
| `src/server/registration.rs` | key loading, Gateway discovery and registration |
| `migrations/`, `sqlx.toml` | Time's schema: tables, checks, keys, indexes and notification triggers |
| `src/persistence/queries/`, `src/tests/queries/` | production and native fixture statements; the workspace's `.sqlx/` holds their checked metadata |

## Verification

`tests/acceptance.rs` qualifies Time through a live local installation and the Gateway:
discovery, tool calls, a schedule Task, future events with resource notifications and
reconciliation after a reconnect, and administration refused to a member and admitted
for an admin. The schedule Task keeps working long enough that its subscription opens on
a Working status. Its completion and result must then arrive as a pushed status
notification, and the test polls the Task only afterwards. The event test likewise reads
Due only after an update that follows its subscription's opening update.
Native Task controls run two process instances on separate pools of the same test
database. They commit cancellation between completion snapshot selection and
settlement, verify remote and local cancellation, preserve completion and calculation
failure outcomes, and reject unrelated conflicts and settlement by an instance that
does not own the Task. A local-token control checks that shutdown without durable
cancel intent leaves the original Task unfinished without a result. Selected-snapshot
controls stop success publication after owner reads and exercise the reconciliation
dispatch seam against actual retained Task state. A claim control shows that a replica
other than the creator claims a queued Task from the stream. A recovery control runs
Time's own consumer on two replicas. The first claims a schedule Task, reports progress
and stalls inside authority loading behind a test lock. The test then ends its instance
lock session, as a crash would. The kernel reaper returns the Task to the queue, the
released worker no longer owns it and settles nothing, and the dead replica's consumer
stops without claiming it again. The second replica's consumer completes the Task with
the output the engine expands directly from the same request. These controls do not
qualify installed cancellation or the restart of an installed process.

Local controls qualify fixture and receipt handling, clock classification, touching
and one-nanosecond interval assertions, and incomplete HTTP response tracing. Installed execution,
Task delivery, restart recovery, authority activation and conflict rollback require
their separate qualification.

Unit tests cover leap authority validation, positive-leap projection, RFC/GPS/DTG
equivalence, DST ambiguity, DST-aware schedule expansion, half-open interval algebra,
timeline violations, clock policy, canonical URIs, acquisition configuration, and
archive traversal rejection.
Runtime tests run against `veoveo-test-fixtures`' PostgreSQL 18 cluster: each test
clones a database with the kernel migrations, applies Time's migrations as
`server_time` and connects two pools as two registration logins. They cover stored
URL/ID admission, SQL isolation and pagination, requested epoch batches and
completion. Administration page cases walk five sources and four acquisitions two at a
time, each record once in its order, and refuse a cursor from another listing.
Completion cases treat `%`, `_` and other characters literally and match only prefixes.
Retained-metadata cases corrupt bodies through fixture statements, verify
read/collection rejection, check that the schema refuses corrupt columns, preserve
lifecycle columns, and reject acquisition identity changes before writing. Isolated authority tests use separate connections to
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
Active-pointer cases show that the schema refuses pointers with a foreign or missing
release, a wrong family, a wrong version or an inconsistent history, and that SQL
excludes a release that is no longer active before its body decodes. Fixture triggers
change pointer and previous-release fields after the candidate update, proving that the
production transaction rechecks them and rolls back all changes. Lock cases hold an
activation open and show that another activation of the tenant and repairs of its
locked rows wait. Registry cases use copied UTC tzdata and temporary leap files. They
qualify persisted selection on a fresh replica, request-time refresh without
notifications, independent epoch maps, cache reuse, failed-load eviction and recovery,
and eviction on a tenant's authority notification and on reconciliation. Notification
cases subscribe one tenant's hub and show that another tenant's event changes never
reach it. Due cases show that an overdue event becomes due once a process starts,
without a read or a subscription, and that two replicas running the statement and
their loops change each event once. A precision case builds a release, an acquisition
and an event from a nanosecond timestamp, reads each back through the database
unchanged, and shows that a direct write of a sub-microsecond timestamp fails.
Permission cases issue access tokens with chosen grants and call every permission
family through the hosted router. The container build and the PostgreSQL fixture
exercise the deployment boundary.

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

## Task Completion Products

Schedule expansion and timeline validation return inline calculations with their
existing typed temporal provenance. These Task completions create no result resource
and omit `result_uri`; MCP envelope admission records that distinction.
