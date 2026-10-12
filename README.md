# veoveo-mcp-time

veoveo-mcp-time is the Veoveo Time MCP server: authority-bound time resolution, calendar expansion, timeline validation and temporal events.

The server lives in `crates/veoveo-mcp-time`, ported from the Veoveo monorepo at main
`0c33cf8a`. Its [design](crates/veoveo-mcp-time/DESIGN.md) and
[agent manual](crates/veoveo-mcp-time/AGENTS.md) describe the domain, tools, Tasks and
storage. It compiles against `veoveo-core`, with every core crate pinned to one revision
in the workspace manifest.

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

## License

Apache-2.0. See [LICENSE](LICENSE).
