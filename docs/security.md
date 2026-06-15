# Security Model

Client repositories may contain untrusted code. `tdc` uses workflow isolation to
reduce routine blast radius:

```text
macOS host
  tdc, VS Code UI, and orchestration

Lima VM
  durable client workspace
  client-scoped GitHub SSH key
  cloned repository
  Docker daemon
  VM snapshots

Devcontainer
  replaceable tool environment
  no GitHub credential by default
  no Docker socket by default
```

This is not a formal sandbox. Snapshots are part of the safety model; create
them before running repo-sourced commands or credentialed operations.

Snapshot availability depends on Lima's VM backend. With Lima 2.1.2 on macOS,
`limactl snapshot` may return `unimplemented` for `vz` VMs. Use `--vm-type qemu`
when creating a VM if snapshot support is required, or `--no-snapshots` if you
intentionally prefer the `vz` backend and accept running without setup
snapshots. The `qemu` backend requires QEMU binaries on `PATH`; on Apple Silicon
that means `qemu-system-aarch64`, typically installed with `brew install qemu`.

The devcontainer profiles:

```text
run as a non-root user
drop Linux capabilities with --cap-drop=ALL
set no-new-privileges
avoid privileged mode
avoid mounting /var/run/docker.sock
avoid mounting SSH keys or GitHub credentials
allow outbound network access
```

Outbound network is allowed because common audit workflows need compiler,
package, and editor tooling downloads.

## VS Code Workspace Trust

VS Code Workspace Trust is still expected in this workflow. Trusting a
workspace allows VS Code and extensions to execute workspace-driven behavior
such as terminals, tasks, debugging, language tooling, and devcontainer startup.

For `tdc`, trust should be scoped to the specific repository folder inside the
client VM:

```text
~/work/<repo>
```

Do not trust `~/work` or a broader parent directory for routine client work.

The trust decision does not move the repository onto the macOS host. The
repository remains inside the Lima VM, and the devcontainer runs from a
`tdc`-generated profile rather than an upstream repository-supplied
devcontainer config.

You can initially open the folder in restricted mode to inspect the repository.
Before using `Dev Containers: Reopen in Container`, trust the specific
repository folder.

## Credentials And Pushes

The default devcontainer profiles do not mount GitHub SSH keys or host
credential agents. Commit and push from the VM shell instead:

```bash
tdc vm ssh --client exampleco
cd ~/work/<repo>
git status
git push
```

This keeps the VM-local client key outside the devcontainer. If a workflow must
push from inside a devcontainer, use only a temporary client-specific
credential and remove or revoke it after use.

## GitHub SSH Host Keys

`tdc` seeds each VM with GitHub's published SSH host keys before cloning over
SSH. This prevents first-use host key prompts while still pinning the expected
GitHub host identity. The packaged keys live at:

```text
assets/payload/ssh/github_known_hosts
```

When GitHub rotates host keys, update that file from GitHub's official SSH key
fingerprint documentation.
