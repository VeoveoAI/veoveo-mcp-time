# veoveo-mcp-time

veoveo-mcp-time is the Veoveo Time MCP server: authority-bound time resolution, calendar expansion, timeline validation and events.

The server lives in `crates/veoveo-mcp-time`. Its [design](crates/veoveo-mcp-time/DESIGN.md)
and [agent manual](crates/veoveo-mcp-time/AGENTS.md) describe the domain, tools, Tasks,
storage and authorization. It compiles against `veoveo-core`, with every core crate
pinned to one revision in the workspace manifest.

## Permissions

Time registers five permissions with the Gateway and checks them again on every request.

| Permission | Grants |
|---|---|
| `time.authorities.read` | `resolve_time`, `convert_time`, `assess_clock`, resources, prompts and completions |
| `time.schedules.expand` | `evaluate_windows` and the `expand_schedule` Task |
| `time.timelines.validate` | the `validate_timeline` Task |
| `time.events.write` | `create_event` and `cancel_event` |
| `time.authorities.administer` | the administrative HTTP surface at `/time/admin` |

Knowledge collections also require the coarse OAuth scope `time`.

## Configuration

Every flag has a matching `VEOVEO_` environment variable.

| Variable | Meaning |
|---|---|
| `VEOVEO_PUBLIC_BASE_URL` | Origin where the Gateway reaches Time. Time serves MCP at `{origin}/time/mcp` and registers that URL. |
| `VEOVEO_GATEWAY_ISSUER` | The Gateway's issuer URL. Time reads its registration endpoint from the issuer's RFC 8414 metadata. |
| `VEOVEO_REGISTRATION_SECRET` | The installation's registration secret, or the placeholder OpenShell substitutes for it. |
| `VEOVEO_KEY_DIRECTORY` | Directory holding Time's registration key; default `/var/lib/veoveo/time/keys`. |
| `VEOVEO_ACQUISITION_SCRATCH_DIRECTORY` | Acquisition workspace; default `/var/lib/veoveo/time/acquisitions`. |
| `VEOVEO_RELEASE_DIRECTORY` | Compiled authority releases; default `/var/lib/veoveo/time/releases`. |
| `VEOVEO_PORT` | Listening port; default `8800`. |
| `VEOVEO_ALLOWED_HOSTS` | Extra comma-separated host authorities that pass host validation. |
| `VEOVEO_NTPD_OBSERVATION_SOCKET` | Optional ntpd-rs observation socket. |

The registration response supplies the access-token issuer and keys and the SurrealDB
endpoint and user, so neither is configured here. Run `veoveo-mcp-time --help` for the
remaining limits.

## Gate

```sh
docker build --target check .
```

The `Dockerfile` defines the `dev` environment, the `check` gate, the release `build` and the `runtime` image.

## Development environment

```sh
docker build --target dev -t veoveo-mcp-time:dev .
openshell sandbox create --name vv-mcp-time --from veoveo-mcp-time:dev \
  --policy openshell/policy.yaml --env HOME=/sandbox --detach -- sleep infinity
```

Sandbox names use the short form `vv-mcp-time`, because OpenShell limits sandbox
names to 19 characters.

### Running Time in the environment

Time runs inside `vv-mcp-time` on port 8800 and registers with a Gateway that the
environment reaches at `http://host.openshell.internal:8080`. The environment's policy
must allow the binary `/sandbox/src/target/debug/veoveo-mcp-time` to reach two
endpoints:

- the Gateway at `host.openshell.internal:8080`, inspected as REST with
  `credential_binding.provider: vv-registration`, so OpenShell substitutes the
  registration secret only there;
- the SurrealDB endpoint the registration response names.

```sh
openshell sandbox exec vv-mcp-time -- bash -lc 'cd /sandbox/src \
  && cargo build --locked --bin veoveo-mcp-time \
  && VEOVEO_PUBLIC_BASE_URL=http://host.openshell.internal:8800 \
     VEOVEO_GATEWAY_ISSUER=http://host.openshell.internal:8080 \
     VEOVEO_KEY_DIRECTORY=/sandbox/time/keys \
     VEOVEO_ACQUISITION_SCRATCH_DIRECTORY=/sandbox/time/acquisitions \
     VEOVEO_RELEASE_DIRECTORY=/sandbox/time/releases \
     target/debug/veoveo-mcp-time'
openshell forward start 8800 vv-mcp-time
```

The `vv-registration` provider supplies `VEOVEO_REGISTRATION_SECRET` to the sandbox as
a placeholder. The forward makes Time reachable from the host on port 8800, where the
Gateway reaches it as `http://host.openshell.internal:8800/time/mcp`. Time removes its
registration when it receives SIGTERM or Ctrl-C.

## License

Apache-2.0. See [LICENSE](LICENSE).
