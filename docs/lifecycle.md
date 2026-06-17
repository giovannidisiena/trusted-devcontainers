# Normal Use Lifecycle

This document describes the expected end-to-end workflow for using `tdc` with an
untrusted client repository.

## 1. Install `tdc`

From a local clone:

```bash
cargo install --path . --locked
```

From a published Git tag:

```bash
cargo install --git https://github.com/<org>/trusted-devcontainers --tag v0.1.0 --locked
```

Cargo installs the `tdc` binary into Cargo's bin directory, usually
`~/.cargo/bin`. Make sure it is on your `PATH`:

```zsh
export PATH="$HOME/.cargo/bin:$PATH"
```

Enable zsh completions:

```bash
mkdir -p ~/.local/share/zsh/site-functions
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
```

Zsh completions include live values for `--client`, `--vm`, and snapshot
`--tag` when `limactl` can list the relevant VMs.

Ensure zsh loads that directory:

```zsh
fpath=("$HOME/.local/share/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

Restart zsh:

```bash
exec zsh -l
```

Install the local manpage for `man tdc`:

```bash
tdc manpage --install
```

If `tdc` prints a `MANPATH` line, add it to your shell config and restart zsh.

## 2. Check Host Prerequisites

Required host tools:

```text
limactl
ssh
rsync
VS Code Remote - SSH extension
VS Code Dev Containers extension
```

Check the host:

```bash
tdc doctor
```

Install QEMU only when using `--vm-type qemu`:

```bash
brew install qemu
```

## 3. Create A Client VM

Recommended fast macOS flow:

```bash
tdc vm new \
  --client exampleco \
  --repo-url https://github.com/cantina-forks/protocol \
  --profile base \
  --vm-type vz \
  --no-snapshots
```

This command:

```text
creates or reuses the Lima VM named client-exampleco
creates a VM-local GitHub SSH key
prints the public key for your GitHub account
seeds GitHub's SSH host keys in the VM
clones the repo into ~/work/protocol
builds the trusted/base:<version> image inside the VM
writes ~/work/protocol/.devcontainer/devcontainer.json
excludes .devcontainer/ from that clone's Git index
```

When prompted, add the printed public key to your personal GitHub account:

```text
GitHub -> Settings -> SSH and GPG keys -> New SSH key
```

Then press Enter in the `tdc` prompt.

## 4. Open In VS Code

Open over Remote SSH:

```bash
tdc vm code --client exampleco --repo protocol
```

VS Code will ask whether you trust the repository. This is expected.

You can choose restricted mode first if you want to inspect the repository
without enabling editor-driven code execution. Before running the devcontainer,
trust the specific repository folder, then run:

```bash
tdc vm code --container --client exampleco --repo protocol
```

The devcontainer open path uses VS Code's Remote Containers URI format. If it
does not open correctly, use `tdc vm code` and then run this from the command
palette:

```text
Dev Containers: Reopen in Container
```

Do not trust `~/work` or a broader parent folder for this workflow. Trust only
the specific repository once you are ready to execute it inside the VM and
devcontainer.

## 5. Day-Two Operation

List VMs:

```bash
tdc vm list
```

Check status:

```bash
tdc vm status --client exampleco
```

Start a stopped VM:

```bash
tdc vm start --client exampleco
```

Open an SSH shell:

```bash
tdc vm ssh --client exampleco
```

Stop a VM:

```bash
tdc vm stop --client exampleco
```

Delete a VM:

```bash
tdc vm delete --client exampleco
```

Use `--force` with `stop` or `delete` if Lima cannot stop or delete the VM
cleanly.

## 6. Snapshots And VM Backends

`vz` is the fast macOS-native backend. With Lima 2.1.2 on macOS,
`limactl snapshot` may return `unimplemented` for `vz` VMs. Use:

```bash
--vm-type vz --no-snapshots
```

when snapshot support is not required.

Use QEMU when setup snapshots are required:

```bash
tdc vm new \
  --client exampleco \
  --repo-url https://github.com/cantina-forks/protocol \
  --profile base \
  --vm-type qemu
```

Create a manual snapshot before risky work:

```bash
tdc vm snapshot create --client exampleco --tag pre-install
tdc vm snapshot list --client exampleco
```

Apply a snapshot to roll back the VM:

```bash
tdc vm snapshot apply --client exampleco --tag pre-install
```

Applying a snapshot stops the VM, restores the snapshot, starts the VM, and
waits for SSH to become available again. Any repo edits made after that
snapshot can be lost unless they were committed, pushed, or exported first.

Delete old snapshots when they are no longer useful:

```bash
tdc vm snapshot delete --client exampleco --tag pre-install
```

VM type is fixed at creation time. Delete and recreate the VM to switch between
`vz` and `qemu`.

## 7. Safe Daily Working Loop

Start the VM:

```bash
tdc vm start --client exampleco
```

Open VS Code over Remote SSH:

```bash
tdc vm code --client exampleco --repo protocol
```

Open the devcontainer:

```bash
tdc vm code --container --client exampleco --repo protocol
```

Inside the devcontainer, inspect first and run dependency installs manually.
For Node projects, start with scripts disabled:

```bash
npm install --ignore-scripts
```

Only run project scripts after review:

```bash
npm run build
npm test
make test
```

Do not add repo-sourced dependency installs or scripts to trusted
`postCreateCommand` hooks for untrusted repositories.

For Codex CLI, install and authenticate in the VM shell rather than inside the
devcontainer:

```bash
tdc vm ssh --client exampleco
curl -fsSL https://chatgpt.com/codex/install.sh | sh
exec bash -l
codex login --device-auth
cd ~/work/protocol
codex
```

`tdc` keeps `~/.local/bin` on the VM user's shell `PATH`, which is where the
Codex standalone installer places the visible `codex` command by default. Keep
Codex credentials outside the devcontainer unless you intentionally accept
exposing them to repository-controlled code.

Before risky commands, create a QEMU snapshot when snapshot support is
available:

```bash
tdc vm snapshot create --client exampleco --tag pre-risky-step-YYYYMMDD
```

Stop the VM when done:

```bash
tdc vm stop --client exampleco
```

## 8. Commit And Push

Preferred model:

```text
edit/build/test inside the devcontainer
commit and push from the VM shell
```

Open a VM shell:

```bash
tdc vm ssh --client exampleco
```

Then push from the VM checkout:

```bash
cd ~/work/protocol
git status
git diff
git add .
git commit -m "Your commit message"
git push
```

This keeps the GitHub credential in the VM instead of exposing it to
repo-sourced code inside the devcontainer. If a workflow truly requires pushing
from inside the devcontainer, expose only a temporary client-specific
credential and remove or revoke it afterward.

## 9. Repair And Partial Setup Commands

The normal `tdc vm new` flow builds the selected image and writes the
devcontainer config. You usually do not need to run these separately.

Check VM, snapshot, and trusted image status:

```bash
tdc vm status --client exampleco
```

Build or rebuild images after `--skip-build` or payload changes:

```bash
tdc images build base --client exampleco
tdc images build solidity-foundry-node --client exampleco
```

`tdc images build` verifies that the expected local image tag exists after the
build. These images are local to the client VM; they are not published registry
images.

Reapply a devcontainer profile:

```bash
tdc devcontainer use \
  --client exampleco \
  --repo protocol \
  --profile base
```

`tdc devcontainer use` checks for the matching local image before writing the
config. If it is missing, the command fails with the exact `tdc images build`
command to run. This keeps VS Code from being the first tool to discover a
missing local image and trying to pull `trusted/...` from a public registry.

The devcontainer config is written to:

```text
~/work/<repo>/.devcontainer
```

If a repository already has its own `.devcontainer` directory, `tdc` refuses to
replace it unless that directory was previously generated by `tdc`.

Rebuild the VS Code devcontainer after changing the profile, image tag, Docker
image, VS Code extensions, or run arguments:

```text
Dev Containers: Rebuild Container
```

or:

```text
Dev Containers: Rebuild and Reopen in Container
```

## 10. Update `tdc`

From a local clone:

```bash
git pull
cargo install --path . --locked
```

When reinstalling the same package version during local development, especially
after changing files under `payload/`, force the reinstall so Cargo replaces the
existing binary:

```bash
cargo install --path . --locked --force
```

From a published Git tag:

```bash
cargo install --git https://github.com/<org>/trusted-devcontainers --tag v0.1.3 --locked
```

Regenerate shell completions after updating:

```bash
tdc completion zsh > ~/.local/share/zsh/site-functions/_tdc
rm -f ~/.zcompdump*
exec zsh -l
```

Existing VMs receive the updated embedded payload the next time a `tdc` command
syncs payload to that VM, such as:

```bash
tdc images build base --client exampleco
tdc devcontainer use --client exampleco --repo protocol --profile base
```

## 11. Cleanup

Delete the VM:

```bash
tdc vm delete --client exampleco --force
```

Remove the GitHub SSH key from your personal GitHub account if you no longer
need that client VM. The key title should match the client slug printed during
setup.

If the VM is retained, remove the repo checkout and VM-local GitHub key:

```bash
tdc repo delete --client exampleco --repo protocol
tdc vm key remove --client exampleco --yes
```

`tdc repo delete` refuses to delete a dirty checkout, a checkout with unpushed
commits, or a checkout without an upstream branch unless `--force` is passed:

```bash
tdc repo delete --client exampleco --repo protocol --force
```

Show the VM-local public key before removing it:

```bash
tdc vm key show --client exampleco
```

Uninstall the host binary:

```bash
cargo uninstall trusted-devcontainers
```
