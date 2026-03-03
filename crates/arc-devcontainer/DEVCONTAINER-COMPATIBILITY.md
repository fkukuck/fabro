# Devcontainer Spec Compatibility Matrix

Compatibility of `arc-devcontainer` with the [devcontainer.json reference](https://containers.dev/implementors/json_reference/).

**Legend**: Yes = fully supported, Partial = parsed but incomplete, No = not supported, Planned = intended for future

## General

| Property | Status | Notes |
|---|---|---|
| `name` | No | Parsed by serde (ignored via `#[serde(default)]`); not exposed in `DevcontainerConfig` |
| `forwardPorts` | Yes | Numeric ports extracted into `DevcontainerConfig::forwarded_ports`; string formats ignored |
| `portsAttributes` | No | Not parsed |
| `otherPortsAttributes` | No | Not parsed |
| `updateRemoteUserUID` | No | Not parsed |
| `containerEnv` | Yes | Baked into generated Dockerfile as `ENV` directives; also exposed in `DevcontainerConfig::container_env` |
| `remoteEnv` | Yes | Merged into `DevcontainerConfig::environment` with variable substitution |
| `containerUser` | Partial | Parsed in `DevcontainerJson::container_user` but not exposed in `DevcontainerConfig` |
| `remoteUser` | Yes | Exposed as `DevcontainerConfig::remote_user` |
| `userEnvProbe` | No | Not parsed |
| `overrideCommand` | Partial | Parsed in `DevcontainerJson::override_command` but not acted on |
| `shutdownAction` | No | Not parsed |

## Image

| Property | Status | Notes |
|---|---|---|
| `image` | Yes | Used as `FROM` line when no Dockerfile is specified; defaults to `mcr.microsoft.com/devcontainers/base:ubuntu` |

## Build (Dockerfile)

| Property | Status | Notes |
|---|---|---|
| `build.dockerfile` | Yes | Resolved relative to devcontainer.json; content read and used as base Dockerfile |
| `build.context` | Yes | Resolved with variable substitution; passed as `DevcontainerConfig::build_context` |
| `build.args` | Yes | Parsed and exposed in `DevcontainerConfig::build_args` for passing to `docker build --build-arg` |
| `build.target` | No | Not parsed |
| `build.cacheFrom` | No | Not parsed |
| `build.options` | No | Not parsed |

## Compose

| Property | Status | Notes |
|---|---|---|
| `dockerComposeFile` | Yes | Single path and array of paths supported; multiple files are merged (last wins for image/build/user; ports accumulate; environment overrides) |
| `service` | Yes | Required when `dockerComposeFile` is set; used to extract service config |
| `runServices` | No | Not parsed; all services assumed |
| `shutdownAction` | No | Not parsed |
| `overrideCommand` | Partial | Parsed but not acted on in compose mode |
| `workspaceFolder` | Yes | Defaults to `/workspaces/{repo-name}` |
| `workspaceMount` | Partial | Parsed in `DevcontainerJson::workspace_mount` but not used |

## Features

| Property | Status | Notes |
|---|---|---|
| `features` | Yes | Fetched via `oras` CLI, topologically sorted by `installsAfter`, Dockerfile layers generated with options as env vars |
| `overrideFeatureInstallOrder` | No | Not parsed |

## Lifecycle

| Property | Status | Notes |
|---|---|---|
| `initializeCommand` | Yes | All three forms supported: string, array, object (parallel). Exposed as `DevcontainerConfig::initialize_commands` |
| `onCreateCommand` | Yes | All three forms supported. Exposed as `DevcontainerConfig::on_create_commands` |
| `updateContentCommand` | No | Not parsed |
| `postCreateCommand` | Yes | All three forms supported. Exposed as `DevcontainerConfig::post_create_commands` |
| `postStartCommand` | Yes | All three forms supported. Exposed as `DevcontainerConfig::post_start_commands` |
| `postAttachCommand` | No | Not parsed |
| `waitFor` | No | Not parsed |

## Host

| Property | Status | Notes |
|---|---|---|
| `hostRequirements` | No | Not parsed |
| `init` | No | Not parsed |
| `privileged` | No | Not parsed |
| `capAdd` | No | Not parsed |
| `securityOpt` | No | Not parsed |
| `mounts` | No | Not parsed |
| `gpuRequest` | No | Not parsed |

## Customizations

| Property | Status | Notes |
|---|---|---|
| `customizations` | No | Unknown fields are silently ignored by serde, so `customizations` is accepted but not processed |

## Variables

| Variable | Status | Notes |
|---|---|---|
| `${localWorkspaceFolder}` | Yes | Substituted via `VariableContext` |
| `${localWorkspaceFolderBasename}` | Yes | Substituted via `VariableContext` |
| `${containerWorkspaceFolder}` | Yes | Substituted via `VariableContext` |
| `${containerWorkspaceFolderBasename}` | Yes | Derived from `containerWorkspaceFolder` by splitting on `/` |
| `${localEnv:VAR}` | Yes | Reads from host environment; supports `:default` syntax |
| `${containerEnv:VAR}` | No | Not implemented (requires running container) |
| `${devcontainerId}` | No | Not implemented |

## JSONC Support

The parser supports JSONC (JSON with Comments):
- Line comments (`//`)
- Block comments (`/* */`)
- Trailing commas before `}` and `]`

## File Discovery

Searched in order:
1. `<path>/.devcontainer/devcontainer.json`
2. `<path>/.devcontainer.json`
3. Direct path if it ends in `devcontainer.json`
