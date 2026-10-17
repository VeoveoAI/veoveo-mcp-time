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

| Variable | Meaning |
|---|---|
| `VEOVEO_PUBLIC_BASE_URL` | Origin where the Hub reaches Time. Time serves MCP at `{origin}/time/mcp` and registers that URL. |
| `VEOVEO_HUB_ISSUER` | The Hub's issuer URL. Time reads its registration endpoint from the issuer's RFC 8414 metadata. |
| `VEOVEO_REGISTRATION_SECRET` | The installation's registration secret, or the placeholder the installation's edge replaces with it. |
| `VEOVEO_KEY_DIRECTORY` | Directory holding Time's registration key; default `/var/lib/veoveo/time/keys`. |
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

Time stores its catalog in the schema `time` of the installation's PostgreSQL 18
database. `migrations/` in the crate defines the schema, and every statement is a
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

A local installation from
[veoveo-template-installation](https://github.com/VeoveoAI/veoveo-template-installation)
runs this image, under its own layer, as the Compose service `vv-mcp-time`, which works
in the installation's Lore repository at `/sandbox/src`. Run the commands below from
the installation's directory.

### Running Time in the environment

Time runs inside `vv-mcp-time` on port 8800 and registers with the Hub at
`http://vv-hub.internal:8080`. The environment holds a placeholder for
`VEOVEO_REGISTRATION_SECRET`, which the installation's edge replaces with the secret on
the registration request, and it reaches PostgreSQL at `vv-postgres.internal:5432`. The
served binary builds into its own target directory, `/sandbox/target-serve`, apart from
`target/`, where the gate and acceptance builds enable test-only dependency features, so
a test build is never served:

```sh
docker compose exec vv-mcp-time bash -lc 'cd /sandbox/src \
  && cargo build --locked --target-dir /sandbox/target-serve --bin veoveo-mcp-time \
  && VEOVEO_PUBLIC_BASE_URL=http://vv-mcp-time.internal:8800 \
     VEOVEO_HUB_ISSUER=http://vv-hub.internal:8080 \
     VEOVEO_KEY_DIRECTORY=/sandbox/time/keys \
     VEOVEO_ACQUISITION_SCRATCH_DIRECTORY=/sandbox/time/acquisitions \
     VEOVEO_RELEASE_DIRECTORY=/sandbox/time/releases \
     exec /sandbox/target-serve/debug/veoveo-mcp-time'
```

The command keeps running in its terminal. Time registers before it listens, and the
Hub reaches it as `http://vv-mcp-time.internal:8800/time/mcp`. Time removes its
registration when it receives SIGTERM or Ctrl-C.

## Acceptance

`tests/acceptance.rs` calls Time through the Hub as the installation's
`acceptance-member` and `acceptance-admin` callers: discovery and tool calls, a refused
unauthenticated request and an undeclared tool, administration refused to the member
and admitted for the admin, a schedule Task whose completion and result arrive as a
pushed status notification, an event whose Due state follows a pushed update of its
resource, and current state after a reconnect. Each subscription opens with the state it
watches, so a test that claims a push reads nothing until a change follows that opening
state. The tests are ignored by default. With the installation running and the callers'
keys copied to `/sandbox/acceptance`, run them:

```sh
docker compose exec vv-mcp-time bash -lc 'cd /sandbox/src \
  && VEOVEO_ACCEPTANCE_HUB=http://vv-hub.internal:8080 \
     VEOVEO_ACCEPTANCE_KEY_DIRECTORY=/sandbox/acceptance \
     cargo nextest run --test acceptance --run-ignored only'
```

## License

Apache-2.0. See [LICENSE](LICENSE).
