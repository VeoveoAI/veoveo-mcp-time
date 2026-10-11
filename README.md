# veoveo-mcp-time

veoveo-mcp-time is the Veoveo Time MCP server: authority-bound time resolution, calendar expansion, timeline validation and temporal events.

This repository is a skeleton created from
[veoveo-template-mcp-rust](https://github.com/VeoveoAI/veoveo-template-mcp-rust). It
owns its toolchain, gate configuration, environment image and OpenShell policy. The
placeholder crate in `crates/veoveo-mcp-time` gives the gates something to check until
Time server code arrives.

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
