# Release Process (GitHub)

## Trigger

Push a semantic version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The `release.yml` workflow will:

1. validate format/lint/tests,
2. build `rango` and `rango-server` for Linux/macOS/Windows targets,
3. create a GitHub Release and upload archives.

## Artifacts

- `rango-linux-x86_64.tar.gz`
- `rango-linux-aarch64.tar.gz`
- `rango-macos-x86_64.tar.gz`
- `rango-macos-aarch64.tar.gz`
- `rango-windows-x86_64.zip`

Each archive contains:

- `rango`
- `rango-server`
