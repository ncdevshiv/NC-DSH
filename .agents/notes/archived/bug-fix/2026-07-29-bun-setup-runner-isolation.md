# Agent Note: Isolate bun setup per GitHub Actions runner

Status: implemented
Archived: 2026-09-04

English | [中文](2026-07-29-bun-setup-runner-isolation.zh.md)

## Problem

`bun/action-setup@v4` defaults its install destination to `~/setup-bun` and replaces that directory during setup. The self-hosted CI failover runs six GitHub Actions runner services under one VM user, so concurrent jobs shared the same destination. In the reproducing run, three jobs entered bun setup within 73 milliseconds; one setup removed another process's current working directory and two jobs failed in Node's `uv_cwd` initialization. A retry on another runner passed, making the failure timing-dependent rather than a repository-test regression.

## Decision

Every non-Windows `bun/action-setup` step in [the primary CI workflow](../../../../.github/workflows/ci.yml) and [the master workflow](../../../../.github/workflows/ci-master.yml) sets `dest: ${{ runner.temp }}/setup-bun-${{ github.run_id }}-${{ github.run_attempt }}`. Each runner service owns its temporary directory, so one setup cannot replace another runner's install directory, and the run/attempt suffix also protects sequential jobs on the same runner from a stale locked `bun.exe`. The Windows native jobs and the [python SDK exe build](../../../../.github/workflows/build-exe-for-python-sdk.yml) use a separate bun executable under `setup-bun-js-${{ github.run_id }}-${{ github.run_attempt }}-${{ github.job }}` (not `standalone: true`; the destination keeps that executable apart): the run/attempt/job suffix gives every job a fresh directory even when sequential jobs land on the same self-hosted runner and a previous job leaves a locked @reflink native module. Persistent store reuse remains separate through `BUN_CONFIG_STORE_DIR`, as established by the [bun provisioning decision](../process/2026-07-26-bun-action-setup-for-symmetric-ci-caching.md).

[The workflow regression test](../../../../scripts/ci-workflow.spec.ts) discovers every `bun/action-setup` step in `ci.yml`, `ci-master.yml`, and `build-exe-for-python-sdk.yml` and rejects one without the runner-private destination. This keeps newly added jobs inside the same isolation boundary.

## Alternatives considered

**Serialize failover jobs.** Rejected because it discards the six-runner pool's intended parallelism and turns an action-local directory collision into queueing across otherwise independent jobs.

**Assign a separate Unix user to every runner service.** This would also separate `HOME`, but it moves the invariant into external VM provisioning and complicates ownership of the deliberately shared persistent bun store. The workflow already receives a runner-private temporary directory.

**Retry failed setup steps.** Rejected because retries only reduce the observed collision rate; another concurrent setup can remove the same shared directory again.

## Consequences

bun's executable installation is ephemeral and isolated per runner, while package downloads still use the configured persistent or cached store. Hosted jobs use the same explicit destination without changing cache policy. The workflow carries three extra configuration lines per setup step, and the regression test must be updated only if bun provisioning intentionally moves to a different isolation mechanism.
