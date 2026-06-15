# Trusted Devcontainers

`tdc` provisions per-client Lima VMs and applies reviewed devcontainer profiles
without requiring this repository to be present after installation.

The installed binary embeds its VM payload: Dockerfiles, devcontainer templates,
VM helper scripts, and tool-version pins. When a command targets a VM, `tdc`
stages that payload in a temporary local directory, syncs it to
`~/trusted-devcontainers` inside the VM, and runs the requested operation there.

## Install

From a clone:

```bash
cargo install --path . --locked
```

From a published Git tag:

```bash
cargo install --git https://github.com/<org>/trusted-devcontainers --tag v0.1.0 --locked
```

This installs the `tdc` binary into Cargo's bin directory, usually
`~/.cargo/bin`. Make sure that directory is on your `PATH`:

```zsh
export PATH="$HOME/.cargo/bin:$PATH"
```

Install zsh completion manually:

```bash
mkdir -p ~/.local/share/zsh/site-functions
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
```

Then make sure your zsh config includes:

```zsh
fpath=("$HOME/.local/share/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

Restart zsh after changing shell configuration:

```bash
exec zsh -l
```

`tdc` does not edit shell startup files during installation.

After updating `tdc`, regenerate the completion file so new commands and flags
show up in zsh:

```bash
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
rm -f ~/.zcompdump*
exec zsh -l
```

See [docs/packaging.md](docs/packaging.md) for release, GitHub Actions, and
versioning steps.

## Prerequisites

On the macOS host:

```text
limactl
ssh
rsync
qemu-system-aarch64, only when using --vm-type qemu on Apple Silicon
VS Code Remote - SSH and Dev Containers extensions, for editor workflow
```

Install QEMU with Homebrew when you need `--vm-type qemu` for snapshot support:

```bash
brew install qemu
```

Check the host:

```bash
tdc doctor
```

## Normal Lifecycle

This is the normal macOS flow for an untrusted client repository.

Create or reuse a client VM, create a VM-local GitHub SSH key, clone a repo,
build the trusted image, and apply the devcontainer config:

```bash
tdc vm new \
  --client exampleco \
  --repo-url https://github.com/cantina-forks/protocol \
  --profile base \
  --vm-type vz \
  --no-snapshots
```

When prompted, add the printed public key to your personal GitHub account:

```text
GitHub -> Settings -> SSH and GPG keys -> New SSH key
```

Then press Enter in the `tdc` prompt. A successful setup prints:

```text
SSH host: lima-client-exampleco
Repo path: ~/work/protocol
```

Open VS Code:

```text
1. Remote-SSH: Connect to Host -> lima-client-exampleco
2. Open folder: ~/work/protocol
3. If prompted, trust only this repository folder when you are ready to run it
4. Dev Containers: Reopen in Container
```

The VS Code trust prompt is expected. You can initially choose restricted mode
to inspect the repository, but Dev Containers requires trusting the folder
before reopening it in a container. Trust the specific repository, not `~/work`
or a broader parent.

In the normal setup flow, you do not need to run `tdc images build` or
`tdc devcontainer use` separately. Those commands are available for repair,
reapply, and intentionally partial setup flows.

See [docs/lifecycle.md](docs/lifecycle.md) for the full normal-use lifecycle,
including QEMU snapshots, updates, cleanup, and repair commands.

## VM Backends

By default, new VMs use Lima's `vz` backend on macOS. On Lima 2.1.2, snapshot
commands may return `unimplemented` for `vz` VMs. For the common fast macOS
path, use `--vm-type vz --no-snapshots`.

If setup snapshots are required, install QEMU and create the VM with
`--vm-type qemu`:

```bash
brew install qemu

tdc vm new \
  --client exampleco \
  --repo-url https://github.com/cantina-forks/protocol \
  --profile base \
  --vm-type qemu
```

VM type is fixed when the Lima VM is created. Existing `vz` VMs cannot be
converted to `qemu`; delete and recreate the VM if you need a different backend.

## Repository Shortcuts

For the common Cantina case, `--repo` uses the default GitHub owner
`cantina-forks`:

```bash
tdc vm new \
  --client exampleco \
  --repo protocol \
  --profile base \
  --vm-type vz \
  --no-snapshots
```

Use a different owner with `--org`:

```bash
tdc vm new \
  --client exampleco \
  --org exampleco \
  --repo protocol \
  --profile base \
  --vm-type vz \
  --no-snapshots
```

## Day-Two Commands

List VMs and inspect status:

```bash
tdc vm list
tdc vm status --client exampleco
```

Start, stop, SSH into, or delete a VM:

```bash
tdc vm start --client exampleco
tdc vm ssh --client exampleco
tdc vm stop --client exampleco
tdc vm delete --client exampleco
```

Use `--force` with `stop` or `delete` if Lima cannot stop or delete the VM
cleanly.

For QEMU-backed VMs, create snapshots around risky work:

```bash
tdc vm snapshot create --client exampleco --tag pre-install
tdc vm snapshot list --client exampleco
```

Roll back or delete snapshots when needed:

```bash
tdc vm snapshot apply --client exampleco --tag pre-install
tdc vm snapshot delete --client exampleco --tag pre-install
```

If you created a VM with `--skip-build`, build the selected profile image before
opening the devcontainer in VS Code:

```bash
tdc images build base --client exampleco
```

Reapply a devcontainer profile after changing profiles or repairing a checkout:

```bash
tdc devcontainer use \
  --client exampleco \
  --repo protocol \
  --profile base
```

Commit and push from the VM shell, not from inside the devcontainer:

```bash
tdc vm ssh --client exampleco
cd ~/work/protocol
git status
git diff
git add .
git commit -m "Your commit message"
git push
```

This keeps the GitHub credential in the VM instead of mounting it into the
devcontainer.

For retained-VM cleanup, remove a repo checkout and VM-local GitHub key:

```bash
tdc repo delete --client exampleco --repo protocol
tdc vm key remove --client exampleco --yes
```

Use `tdc repo delete --force` only after reviewing any uncommitted or unpushed
work.

## Images

Build all trusted images inside a VM:

```bash
tdc images build --client exampleco
```

Build one profile:

```bash
tdc images build solidity-foundry-node --client exampleco
```

Override image namespace or version:

```bash
tdc images build solidity-foundry \
  --client exampleco \
  --namespace trusted \
  --version 0.1.0
```

## Devcontainers

Apply a devcontainer config to a cloned repo inside a VM:

```bash
tdc devcontainer use \
  --client exampleco \
  --repo protocol \
  --profile solidity-foundry
```

The config is written inside the VM under the cloned repository:

```text
~/work/<repo>/.devcontainer
```

`tdc` also adds `.devcontainer/` to the clone's `.git/info/exclude` so the
generated config stays local to the VM checkout. If the repository already has
its own `.devcontainer` directory, `tdc` refuses to replace it unless that
directory was previously generated by `tdc`.

## Profiles

```text
base
  Debian slim, non-root user, Git, SSH client, curl, jq, ripgrep, make.

node
  base + Node.js + npm + corepack/pnpm + native build prerequisites.

solidity-foundry
  base + Foundry + solc-select.

solidity-foundry-node
  solidity-foundry + Node/npm/corepack/pnpm.
```

## Package Layout

```text
src/
  Rust host CLI and orchestration code.

assets/payload/
  Files embedded into the tdc binary and synced into target VMs.
```

Show the manpage:

```bash
tdc manpage
```

Install it for `man tdc`:

```bash
tdc manpage --install
```

Generate raw roff source for packaging:

```bash
tdc manpage --raw > tdc.1
```
