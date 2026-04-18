# Azure Sandbox Design For Fabro

## Summary

Fabro should gain a new `AzureSandbox` provider that combines the container-oriented runtime model of `DockerSandbox` with the remote control-plane model of `DaytonaSandbox`.

For the first Azure branch:

- `fabro-server` runs as a singleton control plane in Azure Container Apps.
- workflow sandboxes run as one Azure Container Instance container group per preserved run.
- sandbox images live in Azure Container Registry and are built by ACR Tasks.
- sandbox workspace state lives at `/workspace`, backed by Azure Files.
- preserve/reconnect targets the same live ACI sandbox by persisted identifier.
- command/file/preview capabilities are exposed through Fabro-owned control-plane glue instead of relying on a single Azure-managed sandbox API.

This preserves Fabro's current workflow semantics while moving hosting and sandbox creation onto Azure primitives.

## Goals

- Run `fabro-server` on Azure as the long-running controller.
- Start isolated workflow sandboxes on Azure without Daytona.
- Preserve Fabro's current remote-sandbox semantics as much as practical:
  - one live sandbox per run
  - in-sandbox git clone/branch/push
  - preserve and reconnect by sandbox identifier
  - workspace rooted at `/workspace`
  - preview URLs as a provider capability
- Reuse existing workflow-engine abstractions rather than redesigning the execution model.
- Prefer a pragmatic Azure-first design over a maximally abstract multi-cloud platform.

## Non-Goals

- Full horizontal scaling of `fabro-server` in the first branch.
- First-class SSH support in the first branch.
- Full parity with Daytona's native snapshot, preview-token, or SSH implementations.
- A generic cloud-provider framework beyond what the current `Sandbox` trait already provides.
- AKS-based sandbox orchestration in the first branch.

## Recommendation

### Chosen Direction

Use Azure Container Apps for `fabro-server` and Azure Container Instances for workflow sandboxes.

This is the best fit for the current Fabro architecture because:

- `fabro-server` is a long-running HTTP controller, which fits Azure Container Apps well.
- ACI is a closer runtime analogue to Fabro's current container-style sandbox behavior than higher-level Azure execution products.
- ACI gives Fabro stable per-sandbox identity, which is required for preserve/reconnect.
- Azure primitives are strong enough for compute, storage, networking, and identity, even though Fabro must build the missing sandbox API surface itself.

### Rejected Alternatives

#### Azure Container Apps Dynamic Sessions

This is the closest managed Azure concept to Daytona, but it was not selected as the primary first-branch target because Fabro currently depends on richer semantics than isolated code execution alone:

- stable preserved sandbox identity
- provider-level preview support
- remote git lifecycle inside the sandbox
- direct file operations

Dynamic Sessions remains a good spike candidate after the ACI path is understood.

#### Azure VMs / VM Scale Sets

This was not selected because it is heavier, slower, and more operationally expensive than container-based sandboxes. It becomes attractive only if first-class SSH or VM-grade machine semantics become hard requirements.

## Azure Topology

### Control Plane

`fabro-server` runs in Azure Container Apps as a singleton controller.

Responsibilities:

- receive API requests
- schedule and coordinate runs
- create, delete, preserve, and reconnect Azure sandboxes
- persist sandbox records in run state
- issue preview tokens and broker preview access
- resolve secrets and provider credentials
- refresh git push credentials for long-lived runs

Initial deployment constraints:

- one active replica
- always-on
- no scale-to-zero

Reason: the current server still stores live run coordination state in-process, including `runs: Mutex<HashMap<RunId, ManagedRun>>` and `scheduler_notify: Notify`, so multi-replica correctness should not be assumed in the first branch.

### Execution Plane

Each workflow run gets one Azure Container Instance container group.

The container group runs:

- the selected sandbox image from ACR
- a mounted `/workspace`
- a Fabro-owned in-sandbox daemon, referred to here as `sandboxd`

Responsibilities of `sandboxd`:

- execute commands with cwd/env/timeout/cancellation semantics
- stream or return command output
- provide remote file operations where needed
- expose a readiness/health endpoint
- optionally provide sandbox metadata and port registration hooks later

### Image Plane

Azure Container Registry stores sandbox images.

ACR Tasks builds images from Dockerfiles.

Fabro's Azure provider treats an ACR image reference as the Azure equivalent of a Daytona snapshot name. The runtime should prefer immutable image references by digest, even if user-facing config continues to use logical names.

### Workspace Plane

Azure Files backs `/workspace`.

The first branch should model `/workspace` as the durable sandbox working tree for:

- repository clone
- workflow edits
- parallel worktrees
- reconnect after control-plane restarts

Fabro artifacts and run/event data should remain in Fabro's durable stores rather than depending on sandbox lifetime.

### Secret and Identity Plane

Use managed identity for `fabro-server`.

`fabro-server` should have permission to:

- manage ACI resources
- pull from ACR
- read required secrets from Key Vault
- access storage required for workspace and artifacts

Sandboxes themselves should not receive broad Azure permissions by default.

## Network Topology

Use a VNet with separate subnets for:

- Azure Container Apps environment
- Azure Container Instances sandboxes
- optional private endpoints

Recommended network shape:

- `fabro-server` can reach Azure APIs and private sandboxes.
- ACI sandboxes do not expose direct public ingress.
- sandbox egress goes through a NAT Gateway initially.
- optional Azure Firewall can be introduced later for stricter egress policy.

This design leaves room to map Daytona-like network modes later:

- `allow_all` -> open outbound through NAT
- `block` -> restricted outbound path
- `allow_list` -> firewall-mediated allow list

The first branch does not need full egress-policy parity.

## Runtime Lifecycle

### Run Start

1. `fabro-server` resolves the Azure sandbox config and image.
2. `fabro-server` ensures the workspace backing exists.
3. `fabro-server` creates an ACI container group.
4. The sandbox starts `sandboxd`.
5. `fabro-server` waits for readiness.
6. Fabro initializes git state inside the sandbox and begins the workflow.

### Preserve and Reconnect

When `preserve = true`, Fabro does not delete the ACI sandbox at finalize.

The persisted sandbox record must contain enough data to reconnect to the exact same live ACI resource, including:

- provider
- working directory
- Azure sandbox identifier
- resource group
- region if needed by the implementation
- any workspace identity required for reconnect

Reconnect flow:

1. `fabro-server` loads the saved sandbox record.
2. It resolves the exact ACI resource.
3. It verifies that the same live sandbox is still running.
4. It reconnects via the Azure provider's remote control channel.

If the live sandbox no longer exists, Fabro should report that clearly rather than pretending the preserved instance is resumable.

## Provider Design

### Core Principle

`AzureSandbox` should combine:

- Docker's image-backed, live-container execution model
- Daytona's remote-provider lifecycle, reconnect, and cloud capabilities

It should not inherit:

- Docker's local daemon and host bind-mount assumptions
- Daytona's SDK-specific implementation details

### Seam-By-Seam Mapping

#### Provider Shape

Use a new remote provider, `AzureSandbox`, exposed via `SandboxSpec::Azure`.

Take from Docker:

- image-first runtime model
- one long-lived container-like sandbox per run
- `/workspace` convention

Take from Daytona:

- provider-specific remote initialization
- preserve/reconnect identity
- server-driven lifecycle

#### Configuration Shape

The Azure provider should get its own runtime config block, analogous to Daytona, carrying Azure-specific settings such as:

- image or logical snapshot name
- cpu / memory
- preserve options
- networking mode
- workspace backing details
- optional image build settings

The top-level `SandboxSpec::Azure` should also carry the same workflow integration inputs currently used by Daytona:

- GitHub app credentials
- run ID
- clone branch
- provider credentials / Azure auth context

#### Initialize / Cleanup

Take from Docker:

- launch one long-lived environment from a selected image

Take from Daytona:

- remote lifecycle and lifecycle events

Azure implementation:

- create/delete ACI container groups
- emit `Initializing`, `Ready`, `CleanupStarted`, `CleanupCompleted`, and failure events

#### Exec Path

Take from Docker:

- execution against a live running container

Take from Daytona:

- remote invocation, timeout handling, and cancellation semantics

Azure implementation:

- `fabro-server` or the provider calls `sandboxd` inside the ACI sandbox
- `sandboxd` provides the equivalent of `exec_command()` for Fabro
- shell-friendly command semantics should be preserved, but implemented by `sandboxd`, not by assuming Azure's native exec facilities are sufficient

#### File Operations

Take from Docker:

- safe content handling and binary-safe upload/download model

Take from Daytona:

- remote file-service semantics

Azure implementation:

- treat `/workspace` as the primary file root backed by Azure Files
- provide sandbox file operations through `sandboxd`
- optionally optimize later for workspace-only direct storage access, but do not make the engine depend on host-path assumptions

#### Snapshot / Image Semantics

Take from Docker:

- image reference is the runtime primitive

Take from Daytona:

- named snapshot lifecycle and snapshot-related events

Azure implementation:

- logical snapshot name resolves to an ACR image reference
- `SnapshotEnsuring` means ensuring the image is available
- `SnapshotCreating` means building it through ACR Tasks when required
- `SnapshotReady` means the image is ready for launch

The first branch should not attempt runtime filesystem capture or Daytona-style mutable snapshot creation.

#### Git Clone / Branch / Push

Take almost entirely from Daytona.

Azure implementation should preserve the existing remote-git pattern:

- clone repository inside the sandbox
- create a run branch in the sandbox
- push from inside the sandbox
- refresh push credentials when tokens expire

This keeps cloud sandbox behavior aligned across Daytona and Azure.

#### Parallel Worktrees

Take from Daytona.

Azure implementation should compute sandbox-local paths for parallel branches under `/workspace/.fabro/scratch/...`, not host-local paths.

#### Preview URLs

Take from Daytona at the trait seam only.

Azure implementation is new:

- sandboxes remain private
- Fabro issues a signed preview URL
- a Fabro preview gateway proxies requests to the sandbox's private IP and requested port

The first branch may defer the full preview gateway, but the provider seam should remain in place.

#### SSH

Take from Daytona at the trait seam only.

Azure implementation should be optional and deferred from the first branch unless it becomes a hard requirement. If SSH is later required, it should be designed explicitly for Azure rather than inferred from Daytona's implementation.

## Codebase Impact

The first branch should add Azure support without destabilizing the existing provider architecture.

Expected additions:

- new Azure sandbox implementation in `lib/crates/fabro-sandbox`
- new Azure config runtime types
- new `SandboxSpec::Azure` variant
- new reconnect path for Azure sandbox records
- workflow config bridge from resolved settings into Azure runtime config
- server-side Azure sandbox orchestration code

Expected reuse:

- the generic `Sandbox` trait
- workflow initialization/finalization lifecycle
- checkpointing flow
- remote git helper functions such as `setup_git_via_exec()` and `git_push_via_exec()`
- preserve/reconnect shape already used by Daytona

Expected new code rather than reuse:

- Azure provider lifecycle calls
- `sandboxd` protocol and implementation
- Azure preview gateway
- Azure-specific sandbox record details

## First Branch Scope

In scope:

- host `fabro-server` on Azure Container Apps
- add `AzureSandbox`
- create ACI sandboxes from ACR images
- mount `/workspace`
- support `initialize`, `cleanup`, `exec_command`, file transfer, git setup, git push, preserve, and reconnect

Out of scope:

- multi-replica controller
- SSH
- full preview gateway polish
- full network-policy parity
- Dynamic Sessions production support

## Risks

### Control Plane Singleton

The current server shape is not obviously horizontally safe. If the first branch accidentally assumes stateless scaling, run coordination will break.

Mitigation: keep `fabro-server` singleton in the first branch.

### Missing Native Azure Sandbox API

Azure does not provide a single managed feature that matches Daytona's combined exec, file, preview, SSH, and reconnect surface.

Mitigation: treat `sandboxd` plus Fabro preview/control-plane glue as first-class design elements rather than incidental helpers.

### Workspace Semantics

Azure Files latency and semantics differ from local disk and Daytona's file APIs.

Mitigation: keep the provider contract remote and abstract; optimize later if hot paths require direct storage access.

### Token Refresh For Long Runs

Git credentials may expire during long preserved runs.

Mitigation: preserve Daytona's `refresh_push_credentials()` pattern in Azure.

## Testing Strategy

The implementation should verify behavior at three levels.

### Unit Tests

- Azure config parsing and bridging
- sandbox record serialization / reconnect metadata
- image resolution and snapshot-name translation
- path computation for parallel worktrees

### Provider-Level Tests

- `AzureSandbox` initialize / cleanup
- remote exec timeout and cancellation semantics
- file upload / download / read / write behavior
- git setup and push helper behavior

These should rely on mocking or narrow provider fakes where possible.

### Azure Integration Tests

- create sandbox from image
- execute a basic command
- clone a repository into `/workspace`
- preserve and reconnect to the same sandbox

Preview and SSH can be tested later when those capabilities are implemented.

## Open Questions Resolved For This Design

- `fabro-server` should live in Azure Container Apps.
- The sandbox runtime should be Azure Container Instances.
- Docker is a conceptual execution model reference, not the Azure deployment target.
- Daytona is a conceptual remote-provider reference, not the Azure implementation basis.

## Implementation Guidance

The intended design center is:

- Docker for runtime shape
- Daytona for remote-provider semantics
- Azure-native primitives for actual hosting

That means `AzureSandbox` should feel operationally like a cloud-hosted, image-backed sandbox with preserve/reconnect, but its implementation should be explicitly Azure-native rather than a thin Daytona emulation layer.
