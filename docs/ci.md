# CI/CD Pipeline

This document describes the GitHub Actions CI/CD pipeline for opensovd.

## Jobs

| Job            | Runs On                | Description                                                                           |
|----------------|------------------------|---------------------------------------------------------------------------------------|
| **prepare**    | Always                 | Entry point; determines release type and whether to run (skips nightly if no changes) |
| **build**      | When `should_run=true` | Builds for Linux, Windows, macOS; runs tests and pytest                               |
| **licenses**   | When `should_run=true` | Checks licenses and sources with cargo-deny                                           |
| **advisories** | When `should_run=true` | Checks security advisories; uploads SARIF on main/nightly                             |
| **lint**       | When `should_run=true` | Runs rustfmt, clippy, and pre-commit hooks (prek)                                     |
| **coverage**   | When `should_run=true` | Generates coverage report, deploys to GitHub Pages on main                            |
| **docker**     | main/tags/schedule     | Builds and pushes Docker images (gateway, mcp) to GHCR                                |
| **release**    | main/tags/schedule     | Creates GitHub release with artifacts and changelog                                   |
| **gate**       | Always                 | Final check that all jobs passed (use for branch protection)                          |

## Dependency Chain

```mermaid
flowchart TB
    prepare
    subgraph parallel[ ]
        direction LR
        build
        licenses
        advisories
        lint
        coverage
    end
    prepare --> build & licenses & advisories & lint & coverage
    build --> docker & release
    docker & release & licenses & advisories & lint & coverage --> gate
```

Jobs `build`, `licenses`, `advisories`, `lint`, and `coverage` run in parallel after `prepare`.

## Nightly Skip Logic

The `prepare` job compares the current SHA with the `nightly` tag. If unchanged, it sets `should_run=false` and all downstream jobs are skipped, saving CI resources.

## Release Tags

| Tag       | Trigger            | Channel   | Description                                     |
|-----------|--------------------|-----------|-------------------------------------------------|
| `latest`  | Push to main       | `dev`     | Latest successful main branch build             |
| `nightly` | Daily at 02:00 UTC | `nightly` | Scheduled nightly build (skipped if no changes) |
| `vX.Y.Z`  | Tag push           | `stable`  | Versioned production release                    |

## Release Channels

The channel determines the version suffix the binaries report through `--version`,
the SOVD vendor info and the MCP handshake. Only `stable` ships unsuffixed:

| Channel   | Version string  | Built by                             |
|-----------|-----------------|--------------------------------------|
| `stable`  | `0.1.1`         | Tag push, after the tag is validated |
| `nightly` | `0.1.1-nightly` | Scheduled build                      |
| `dev`     | `0.1.1-dev`     | main, pull requests, local builds    |

The suffixes are semver pre-releases, so `0.1.1-dev` sorts before `0.1.1-nightly`,
which sorts before `0.1.1`.

The `prepare` job derives the channel from the release type and passes it to
`build`. Downstream jobs test `channel != 'stable'` wherever they need to know
whether a build is a prerelease. Three environment variables feed the build scripts, all optional and all
defaulting to a dev build stamped from the local git checkout:

| Variable              | Purpose                                                          |
|-----------------------|------------------------------------------------------------------|
| `OPENSOVD_CHANNEL`    | Release channel; an unrecognised value fails the build            |
| `OPENSOVD_COMMIT_SHA` | Revision to stamp, for builds without a git checkout              |
| `SOURCE_DATE_EPOCH`   | Build date to stamp, for reproducible builds                      |

On a tag push `prepare` verifies that the tag matches the workspace version and
fails the run on a mismatch, so a `v0.2.0` tag cannot ship binaries reporting
`0.1.1`.

## Docker Tags

| Tag                  | When Created        | Description                            |
|----------------------|---------------------|----------------------------------------|
| `latest`             | main or version tag | Points to the most recent stable build |
| `nightly`            | Scheduled build     | Latest nightly build                   |
| `nightly-YYYY-MM-DD` | Scheduled build     | Date-stamped nightly build             |
| `vX.Y.Z`             | Version tag push    | Specific version release               |

## Branch Protection

Use the `gate` job as the required status check for branch protection rules.
