# veoveo-mcp-time

veoveo-mcp-time is the Veoveo Time MCP server: authority-bound time resolution, calendar expansion, timeline validation and events.

The server lives in `crates/veoveo-mcp-time`. Its [design](crates/veoveo-mcp-time/DESIGN.md)
and [agent manual](crates/veoveo-mcp-time/AGENTS.md) describe the domain, tools, Tasks,
storage and authorization. It serves MCP through `veoveo-hub-sdk` and stores its
state through `veoveo-core`, with each repository's crates pinned to one revision in
the workspace manifest.

## Permissions

Time registers nine permissions with the Hub and checks them again on every request.
The administrative ones carry the `administer` risk, so only roles that allow it grant them.

| Permission | Grants |
|---|---|
| `time.authorities.read` | `resolve_time`, `convert_time`, `assess_clock`, resources, prompts and completions |
| `time.schedules.expand` | `evaluate_windows` and the `expand_schedule` Task |
| `time.timelines.validate` | the `validate_timeline` Task |
| `time.events.write` | `create_event` and `cancel_event` |
| `time.authorities.inspect` | `list_sources`, `get_source`, `list_acquisitions`, `get_acquisition` and `list_active_authorities` |
| `time.authorities.administer` | `create_source`, `replace_source`, `start_acquisition`, `cancel_acquisition` and `activate_release` |
| `time.calendars.write` | `create_calendar` |
| `time.epochs.write` | `create_epoch` |
| `time.clock-policies.write` | `replace_clock_policy` |

## Configuration

Every flag has a matching `VEOVEO_` environment variable.
The required `VEOVEO_DEPLOYMENT_ID` is a UUIDv7 shared by registration and Task
execution; Hub supplies it with the registration token. Preserve it and the private
key directory on ordinary restart.

Time registers with `VEOVEO_REGISTRATION_SECRET` on its first start and resumes its
saved registration afterwards; process stop does not retire it. When its
declarations change, or the Hub no longer knows the saved registration, Time
registers again with the same token and replaces the saved state.

To print the public registration metadata offline:

```sh
veoveo-mcp-time --describe-registration \
  --deployment-id <allocated-uuidv7> \
  --public-base-url https://candidate.example \
  --key-directory /sandbox/state/time/keys
```

This reuses the key in `--key-directory` and neither registers nor connects to Hub
or the database.

| Variable | Meaning |
|---|---|
| `VEOVEO_DEPLOYMENT_ID` | Required stable UUIDv7 of this admitted deployment; shared by registration, trust and Task execution. |
| `VEOVEO_PUBLIC_BASE_URL` | Origin where the Hub reaches Time. Time serves MCP at `{origin}/time/mcp` and registers that URL. |
| `VEOVEO_HUB_ISSUER` | The Hub's issuer URL. Time reads its registration endpoint from the issuer's RFC 8414 metadata. |
| `VEOVEO_REGISTRATION_SECRET` | Namespace initial access or deployment-scoped admission (possibly a native opaque reference), used only for initial registration; resume uses saved native credentials. |
| `VEOVEO_KEY_DIRECTORY` | Private directory holding the deployment's key and saved native registration; default `/var/lib/veoveo/time/keys`. |
| `VEOVEO_ACQUISITION_SCRATCH_DIRECTORY` | Acquisition workspace; default `/var/lib/veoveo/time/acquisitions`. |
| `VEOVEO_RELEASE_DIRECTORY` | Compiled authority releases; default `/var/lib/veoveo/time/releases`. |
| `VEOVEO_PORT` | Listening port; default `8800`. |
| `VEOVEO_ALLOWED_HOSTS` | Extra comma-separated host authorities that pass host validation. |
| `VEOVEO_NTPD_OBSERVATION_SOCKET` | Optional ntpd-rs observation socket. |

Time answers liveness at `/livez` and readiness at `/readyz` on its listening port,
outside the `/time` prefix, for example `http://<host>:8800/livez`. Readiness holds while
Time observes its clock.

The registration response supplies the access-token issuer and keys and the PostgreSQL
login Time connects with, so neither is configured here. Time applies its own
migrations in the schema `time` at startup. Run `veoveo-mcp-time --help` for the
remaining limits.

## Database

Time stores its catalog in the schema `time` of the PostgreSQL 18 database its
registration grant names. `migrations/` in the crate defines the schema, and every statement is a
`.sql` file that sqlx checks at compile time. `.cargo/config.toml` sets `SQLX_OFFLINE`,
so every build uses the committed `.sqlx` metadata. After changing a query or a
migration, regenerate it against a database with the kernel migrations of the pinned
veoveo-core and Time's migrations applied:

```sh
CARGO_TARGET_DIR=<empty directory> cargo sqlx prepare --workspace -- --all-targets --all-features
```

The metadata also covers the queries of the core crates, and prepare records only the
crates it compiles, so it needs a target directory with no earlier build.

The `check` stage of the `Dockerfile` builds that database and fails when the metadata
is stale. Tests need the PostgreSQL 18 server programs on `PATH`; each test clones its
own database from a migrated template.

## Gate

```sh
docker build --target check .
```

The `Dockerfile` defines the `dev` environment, the `check` gate, the release `build` and the `runtime` image.

## Development environment

```sh
docker build --target dev -t veoveo-mcp-time:dev .
```

## Serving

Time is served from its factory sandbox, as its
[design](crates/veoveo-mcp-time/DESIGN.md#deployment) describes: a pushed branch of
Time's Lore repository that has `openshell/run` gets one sandbox, which the Hub's
[factory](https://github.com/VeoveoAI/veoveo-hub/blob/main/crates/veoveo-hub/DESIGN.md#factory-sandboxes)
builds from `openshell/image/` and runs. `openshell/run` builds the release binary from
the checkout and becomes the server, with its state on the sandbox's disk under
`/sandbox/state/time`. The Hub supplies the deployment ID, public base URL, port,
issuer and registration placeholder, and reaches Time only through OpenShell's
`ForwardTcp` to that port; Time needs no public exposure. Every push of the branch
rebuilds and restarts it, and the person who pushed chooses the branch they use. The
installation's part is its
[factory section](https://github.com/VeoveoAI/veoveo-template-installation/blob/main/DESIGN.md#factory);
the installation runs no Time sandbox of its own.

The root `Dockerfile`'s `runtime` image holds the same binary for a deployment outside a
factory; it reads its state from `/var/lib/veoveo/time`.

## Acceptance

`tests/acceptance.rs` calls Time through the Hub as the installation's
member and admin identities: discovery and tool calls, a refused
unauthenticated request and an undeclared tool, administration refused to the member
and admitted for the admin, a schedule Task whose completion and result arrive as a
pushed status notification, a Working schedule Task cancelled from another member
connection with a pushed and retained Cancelled outcome, an event whose Due state
follows a pushed update of its resource, and current state after a reconnect. Each subscription opens with the
state it watches, so a test that claims a push reads nothing until a change follows that opening
state. The unauthenticated control sends a valid MCP `2026-07-28` envelope and requires
HTTP 401 with a Bearer challenge from Hub; native policy denial or transport failure
cannot satisfy it. Cancellation requires a pushed terminal state, not just a cancel
acknowledgement. The live tests are ignored by default. The trusted qualification runner
receives separate native member and admin bearer references; it does not mint tokens
or load caller signing keys. Set selectors to the names of the injected variables:

```sh
export VEOVEO_ACCEPTANCE_HUB=https://<native-hub-hostname>:<gateway-port>
export VEOVEO_ACCEPTANCE_MEMBER_BEARER_ENV=HUB_MEMBER_REFERENCE
export VEOVEO_ACCEPTANCE_ADMIN_BEARER_ENV=HUB_ADMIN_REFERENCE
cargo nextest run --test acceptance --run-ignored only -E 'not (test(restart_persistence_) or test(a_running_schedule_task_recovers_after_restart))'
```

`HUB_MEMBER_REFERENCE` and `HUB_ADMIN_REFERENCE` must already contain their distinct
nonempty bearer/opaque references. Missing inputs, ambiguous member/admin inputs
and the retired `VEOVEO_ACCEPTANCE_KEY_DIRECTORY` fail explicitly without printing
credentials. A member-only runner may omit the admin selector and run the member
cases; the full suite requires both identities. No runner receives private caller
keys.

The pinned reqwest transport uses platform certificate verification. In Linux
sandboxes, provision the workload/interception CA bundle through the supported
`SSL_CERT_FILE` (or `SSL_CERT_DIR`) input before constructing clients. Both rmcp and
the unauthenticated control request share that verified client; do not disable TLS
verification or replace the logical Hub hostname. A static opaque reference relies
on native provider renewal; a raw bearer is not refreshed by this test helper.
Credential refresh and deployed permissions still require live qualification.
These cases do not restart Time or Hub. Retained registration identity, durable
event/Task state across actual restarts and recovery of in-flight Tasks require
separate trusted operator orchestration; the domain runner has no management
credentials.

Input selection tests run without a live installation or process-environment changes:

```sh
cargo test --locked -p veoveo-mcp-time --test acceptance bearer_inputs
```

## License

Apache-2.0. See [LICENSE](LICENSE).

### Restart persistence phases

Run these phases separately through the same member identity. Preparation creates
and cancels one event and a real schedule Task, checks their retained outcomes,
and prints the event URI/version and Task ID/status as four non-secret environment
assignments:

```sh
cargo test --locked -p veoveo-mcp-time --test acceptance restart_persistence_prepare -- --ignored --exact --nocapture
```

The trusted operator records Time's registration public identity/key thumbprint
and native runtime IDs, then restarts Time and Hub using their native lifecycle,
retaining the Time volume and database. The operator separately proves the actual
restarts, registration identity and readiness. This test process has no management
credential. Export the four printed assignments unchanged, then run:

```sh
cargo test --locked -p veoveo-mcp-time --test acceptance restart_persistence_verify -- --ignored --exact --nocapture
```

Verification requires that exact event to remain Cancelled at the prepared version
and that exact Task to retain its Cancelled payload, then opens fresh event and
Task subscriptions and reconciles their initial deliveries. These are retained
terminal-state checks; they do not establish recovery of an in-flight Task or prove
that a restart happened. Keep both phases out of the ordinary live suite and run
them only around the operator-controlled boundary. Do not put opaque references or
private registration material into the printed assignments.

### In-flight Task restart

Run this explicit case **alone**, through native Exec with an open streamed stdin
pipe and stdout forwarding. Use the same trusted qualification runner's member
reference; it receives no management credentials:

```sh
cargo test --locked -p veoveo-mcp-time --test acceptance a_running_schedule_task_recovers_after_restart -- --ignored --exact --nocapture
```

The runner creates one finite historical schedule, observes Working, prints its
non-secret Task ID and readiness line, and flushes stdout. Keep stdin open. After
that line, the trusted operator proves that the old Time runtime stopped, restarts
Time with the retained volume/database, and proves the new runtime is ready.
Only then send the exact line `continue` followed by LF over the existing Exec
stdin stream. Native `ExecSandboxInteractive` supports this boundary; the buffered
SDK `exec` convenience method does not provide interactive output. The CLI's
`sandbox exec --no-tty` supports an open stdin pipe. No file or manifest mediates
the boundary.

The same Task must still be Working on a fresh connection after continuation,
then its fresh subscription must open Working and push Completed. Its typed result
must equal the expected 64 UTC windows and its completion must remain readable.
Any early Completed observation fails explicitly as inconclusive restart evidence.
The operator's runtime evidence remains required; the runner does not restart a
service or infer a restart from a disconnect or timestamp. The whole case has a
240-second deadline and at most ten additional seconds for cancellation cleanup.
Missing continuation cannot hold the test process open through Tokio stdin.
