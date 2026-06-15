use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::CommandFactory;
use tempfile::Builder;

use crate::cli::{
    Cli, Command as TdcCommand, DevcontainerCommand, DevcontainerUseArgs, ImagesBuildArgs,
    ImagesCommand, ManpageArgs, RepoCommand, RepoDeleteArgs, VmClientTargetArgs, VmCommand,
    VmDeleteArgs, VmKeyCommand, VmKeyRemoveArgs, VmNewArgs, VmSnapshotArgs, VmSnapshotCommand,
    VmSnapshotTagArgs, VmStopArgs, VmTargetArgs,
};
use crate::github::{self, RepoSpec};
use crate::model::{self, Profile};
use crate::{payload, process};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        TdcCommand::Vm(args) => match args.command {
            VmCommand::New(args) => vm_new(args),
            VmCommand::List => vm_list(),
            VmCommand::Snapshot(args) => vm_snapshot(args),
            VmCommand::Key(args) => match args.command {
                VmKeyCommand::Show(args) => vm_key_show(args),
                VmKeyCommand::Remove(args) => vm_key_remove(args),
            },
            VmCommand::Ssh(args) => vm_ssh(args),
            VmCommand::Status(args) => vm_status(args),
            VmCommand::Start(args) => vm_start(args),
            VmCommand::Stop(args) => vm_stop(args),
            VmCommand::Delete(args) => vm_delete(args),
        },
        TdcCommand::Repo(args) => match args.command {
            RepoCommand::Delete(args) => repo_delete(args),
        },
        TdcCommand::Images(args) => match args.command {
            ImagesCommand::Build(args) => images_build(args),
        },
        TdcCommand::Devcontainer(args) => match args.command {
            DevcontainerCommand::Use(args) => devcontainer_use(args),
        },
        TdcCommand::Completion(args) => completion(args.shell),
        TdcCommand::Manpage(args) => manpage(args),
        TdcCommand::Doctor => doctor(),
    }
}

fn completion(shell: clap_complete::Shell) -> Result<()> {
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "tdc", &mut io::stdout());
    Ok(())
}

fn manpage(args: ManpageArgs) -> Result<()> {
    ensure!(
        !args.raw || !args.install,
        "--raw and --install cannot be used together"
    );

    let manpage = render_manpage()?;

    if args.install {
        install_manpage(&manpage, args.install_dir.as_deref())?;
        return Ok(());
    }

    if args.raw || !io::stdout().is_terminal() {
        io::stdout()
            .write_all(&manpage)
            .context("failed to write manpage")?;
        return Ok(());
    }

    if !process::command_exists("man") {
        io::stdout()
            .write_all(&manpage)
            .context("failed to write manpage")?;
        return Ok(());
    }

    show_manpage(&manpage)
}

fn render_manpage() -> Result<Vec<u8>> {
    let command = Cli::command();
    let mut output = Vec::new();
    clap_mangen::Man::new(command)
        .render(&mut output)
        .context("failed to render manpage")?;
    Ok(output)
}

fn show_manpage(manpage: &[u8]) -> Result<()> {
    let temp = Builder::new()
        .prefix("tdc-manpage-")
        .tempdir()
        .context("failed to create manpage staging directory")?;
    let man1 = temp.path().join("man1");
    fs::create_dir_all(&man1).with_context(|| format!("failed to create {}", man1.display()))?;
    let target = man1.join("tdc.1");
    fs::write(&target, manpage).with_context(|| format!("failed to write {}", target.display()))?;

    process::run({
        let mut command = Command::new("man");
        command.arg("-M").arg(temp.path()).arg("tdc");
        command
    })
}

fn install_manpage(manpage: &[u8], install_dir: Option<&Path>) -> Result<()> {
    let base = match install_dir {
        Some(path) => expand_home(path)?,
        None => home_dir()?.join(".local/share/man"),
    };
    let man1 = if base.file_name().is_some_and(|name| name == "man1") {
        base.clone()
    } else {
        base.join("man1")
    };
    let manpath_dir = if man1.file_name().is_some_and(|name| name == "man1") {
        man1.parent().unwrap_or(&man1).to_path_buf()
    } else {
        man1.clone()
    };
    let target = man1.join("tdc.1");

    fs::create_dir_all(&man1).with_context(|| format!("failed to create {}", man1.display()))?;
    fs::write(&target, manpage).with_context(|| format!("failed to write {}", target.display()))?;

    println!("Installed manpage: {}", target.display());
    if manpath_contains(&manpath_dir) {
        println!("Try: man tdc");
    } else {
        println!();
        println!("Add this to your shell config so `man tdc` can find it:");
        println!("  export MANPATH=\"{}:$MANPATH\"", manpath_dir.display());
        println!();
        println!("Then restart zsh:");
        println!("  exec zsh -l");
    }

    Ok(())
}

fn expand_home(path: &Path) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }
    Ok(path.to_path_buf())
}

fn manpath_contains(path: &Path) -> bool {
    let needle = canonical_or_original(path);
    let output = Command::new("manpath").output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .map(|entry| canonical_or_original(&entry))
        .any(|entry| entry == needle)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn doctor() -> Result<()> {
    let required = ["limactl", "ssh", "rsync"];
    for command in required {
        if process::command_exists(command) {
            println!("ok: {command}");
        } else {
            println!("missing: {command}");
        }
    }

    process::require_commands(&required)?;
    println!("ok: packaged payload {}", payload::packaged_version()?);
    Ok(())
}

fn vm_new(args: VmNewArgs) -> Result<()> {
    process::require_commands(&["limactl", "ssh", "rsync"])?;
    ensure!(
        model::is_valid_slug(&args.client),
        "invalid client slug: {}",
        args.client
    );

    let repo = github::resolve_repo_input(
        &args.repo.org,
        args.repo.repo.as_deref(),
        args.repo.repo_url.as_deref(),
    )?;
    let vm = args.vm.unwrap_or_else(|| model::vm_default(&args.client));
    ensure!(model::is_valid_slug(&vm), "invalid VM name: {vm}");

    let host = model::lima_host(&vm);
    let key_name = github_key_name(&args.client);
    let key_comment = format!("{} client GitHub key", args.client);

    ensure_vm_type_prerequisites(args.vm_type.as_str())?;
    ensure_host_ssh_include()?;
    start_vm(
        &vm,
        args.vm_type.as_str(),
        args.cpus,
        args.memory,
        args.disk,
    )?;
    wait_for_ssh(&host)?;

    payload::sync_to_vm(&host)?;
    setup_vm_github_key(&host, &args.client, &key_name, &key_comment)?;
    print_key_and_maybe_wait(&host, &key_name, !args.no_prompt)?;

    if !args.skip_clone {
        clone_repo_in_vm(&host, &repo.repo, &repo.clone_url)?;
        if !args.no_snapshots {
            snapshot_vm(&vm, "clean-clone")?;
        }
    }

    if !args.skip_build {
        build_images_on_vm(&host, args.profile.as_str(), "trusted", None)?;
    }

    if !args.skip_clone {
        apply_devcontainer_in_vm(&host, &repo.repo, &args.profile)?;
        if !args.no_snapshots {
            snapshot_vm(&vm, "trusted-devcontainer-ready")?;
        }
    }

    print_next_steps(
        &args.client,
        &repo,
        &vm,
        &host,
        &args.profile,
        args.skip_build,
    );
    Ok(())
}

fn vm_list() -> Result<()> {
    process::require_commands(&["limactl"])?;

    let output = Command::new("limactl")
        .arg("list")
        .output()
        .context("failed to start limactl list")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            println!("No VMs found.");
        } else {
            print!("{stdout}");
        }
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "limactl list exited with {}: {}",
        output.status,
        stderr.trim()
    )
}

fn vm_snapshot(args: VmSnapshotArgs) -> Result<()> {
    match args.command {
        VmSnapshotCommand::List(args) => vm_snapshot_list(args),
        VmSnapshotCommand::Create(args) => vm_snapshot_create(args),
        VmSnapshotCommand::Apply(args) => vm_snapshot_apply(args),
        VmSnapshotCommand::Delete(args) => vm_snapshot_delete(args),
    }
}

fn vm_snapshot_list(args: VmTargetArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(&args, "tdc vm snapshot list [--client <CLIENT>|--vm <VM>]")?;
    list_snapshots(&vm)
}

fn vm_snapshot_create(args: VmSnapshotTagArgs) -> Result<()> {
    process::require_commands(&["limactl", "ssh"])?;
    let vm = target_vm(
        &args.target,
        "tdc vm snapshot create --tag <TAG> [--client <CLIENT>|--vm <VM>]",
    )?;
    snapshot_vm(&vm, &args.tag)
}

fn vm_snapshot_apply(args: VmSnapshotTagArgs) -> Result<()> {
    process::require_commands(&["limactl", "ssh"])?;
    let vm = target_vm(
        &args.target,
        "tdc vm snapshot apply --tag <TAG> [--client <CLIENT>|--vm <VM>]",
    )?;
    apply_snapshot(&vm, &args.tag)
}

fn vm_snapshot_delete(args: VmSnapshotTagArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(
        &args.target,
        "tdc vm snapshot delete --tag <TAG> [--client <CLIENT>|--vm <VM>]",
    )?;

    process::run({
        let mut command = Command::new("limactl");
        command
            .arg("snapshot")
            .arg("delete")
            .arg(&vm)
            .arg("--tag")
            .arg(&args.tag);
        command
    })
}

fn vm_key_show(args: VmClientTargetArgs) -> Result<()> {
    process::require_commands(&["ssh"])?;
    let vm = client_vm(&args.client, args.vm.as_deref())?;
    let host = model::lima_host(&vm);
    let key_name = github_key_name(&args.client);

    process::run({
        let mut command = Command::new("ssh");
        command.arg(host).arg(format!("cat ~/.ssh/{key_name}.pub"));
        command
    })
}

fn vm_key_remove(args: VmKeyRemoveArgs) -> Result<()> {
    process::require_commands(&["ssh"])?;
    if !args.yes {
        bail!(
            "refusing to remove VM-local GitHub key without --yes\n\nUsage: tdc vm key remove --client <CLIENT> [--vm <VM>] --yes"
        );
    }

    let vm = client_vm(&args.target.client, args.target.vm.as_deref())?;
    let host = model::lima_host(&vm);
    remove_vm_github_key(
        &host,
        &args.target.client,
        &github_key_name(&args.target.client),
    )
}

fn vm_ssh(args: VmTargetArgs) -> Result<()> {
    process::require_commands(&["ssh"])?;
    let vm = target_vm(&args, "tdc vm ssh [--client <CLIENT>|--vm <VM>]")?;
    process::run({
        let mut command = Command::new("ssh");
        command.arg(model::lima_host(&vm));
        command
    })
}

fn vm_status(args: VmTargetArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(&args, "tdc vm status [--client <CLIENT>|--vm <VM>]")?;

    process::run({
        let mut command = Command::new("limactl");
        command.arg("list").arg(&vm);
        command
    })?;

    print_snapshot_status_if_available(&vm)?;

    Ok(())
}

fn print_snapshot_status_if_available(vm: &str) -> Result<()> {
    let output = Command::new("limactl")
        .arg("snapshot")
        .arg("list")
        .arg(vm)
        .output()
        .context("failed to start limactl snapshot list")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            print!("{stdout}");
        }
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("unimplemented") {
        eprintln!("Snapshots: unavailable with this limactl snapshot backend");
    } else {
        eprintln!(
            "warning: failed to list snapshots: {}",
            stderr.trim().lines().last().unwrap_or("unknown error")
        );
    }

    Ok(())
}

fn list_snapshots(vm: &str) -> Result<()> {
    let output = Command::new("limactl")
        .arg("snapshot")
        .arg("list")
        .arg(vm)
        .output()
        .context("failed to start limactl snapshot list")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{stdout}");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("unimplemented") {
        bail!(
            "snapshots unavailable with this limactl snapshot backend; recreate the VM with --vm-type qemu for snapshot support"
        );
    }

    bail!(
        "limactl snapshot list {vm} exited with {}: {}",
        output.status,
        stderr.trim()
    )
}

fn vm_start(args: VmTargetArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(&args, "tdc vm start [--client <CLIENT>|--vm <VM>]")?;

    process::run({
        let mut command = Command::new("limactl");
        command.arg("start").arg(vm);
        command
    })
}

fn vm_stop(args: VmStopArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(&args.target, "tdc vm stop [--client <CLIENT>|--vm <VM>]")?;

    process::run({
        let mut command = Command::new("limactl");
        command.arg("stop");
        if args.force {
            command.arg("--force");
        }
        command.arg(vm);
        command
    })
}

fn vm_delete(args: VmDeleteArgs) -> Result<()> {
    process::require_commands(&["limactl"])?;
    let vm = target_vm(&args.target, "tdc vm delete [--client <CLIENT>|--vm <VM>]")?;

    process::run({
        let mut command = Command::new("limactl");
        command.arg("delete").arg("--yes");
        if args.force {
            command.arg("--force");
        }
        command.arg(vm);
        command
    })
}

fn images_build(args: ImagesBuildArgs) -> Result<()> {
    process::require_commands(&["ssh", "rsync"])?;
    let vm = target_vm(
        &args.target,
        "tdc images build [--client <CLIENT>|--vm <VM>] [PROFILE]",
    )?;
    let host = model::lima_host(&vm);

    ensure_host_ssh_include()?;
    payload::sync_to_vm(&host)?;
    build_images_on_vm(
        &host,
        args.profile.as_str(),
        &args.namespace,
        args.version.as_deref(),
    )
}

fn devcontainer_use(args: DevcontainerUseArgs) -> Result<()> {
    process::require_commands(&["ssh", "rsync"])?;
    ensure!(
        model::is_valid_slug(&args.client),
        "invalid client slug: {}",
        args.client
    );

    let repo = github::resolve_repo_input(
        &args.repo.org,
        args.repo.repo.as_deref(),
        args.repo.repo_url.as_deref(),
    )?;
    let vm = args.vm.unwrap_or_else(|| model::vm_default(&args.client));
    ensure!(model::is_valid_slug(&vm), "invalid VM name: {vm}");
    let host = model::lima_host(&vm);

    ensure_host_ssh_include()?;
    payload::sync_to_vm(&host)?;
    apply_devcontainer_in_vm(&host, &repo.repo, &args.profile)
}

fn repo_delete(args: RepoDeleteArgs) -> Result<()> {
    process::require_commands(&["ssh"])?;
    ensure!(
        model::is_valid_slug(&args.client),
        "invalid client slug: {}",
        args.client
    );

    let repo = github::resolve_repo_input(
        &args.repo.org,
        args.repo.repo.as_deref(),
        args.repo.repo_url.as_deref(),
    )?;
    let vm = client_vm(&args.client, args.vm.as_deref())?;
    let host = model::lima_host(&vm);

    delete_repo_in_vm(&host, &repo.repo, args.force)
}

fn target_vm(args: &VmTargetArgs, usage: &str) -> Result<String> {
    if args.client.is_some() && args.vm.is_some() {
        bail!("the argument '--client <CLIENT>' cannot be used with '--vm <VM>'\n\nUsage: {usage}");
    }

    if let Some(client) = &args.client {
        ensure!(
            model::is_valid_slug(client),
            "invalid client slug: {client}"
        );
    }
    if let Some(vm) = &args.vm {
        ensure!(model::is_valid_slug(vm), "invalid VM name: {vm}");
        return Ok(vm.clone());
    }
    if let Some(client) = &args.client {
        return Ok(model::vm_default(client));
    }

    bail!(
        "the following required arguments were not provided:\n  --client <CLIENT> or --vm <VM>\n\nUsage: {usage}"
    )
}

fn client_vm(client: &str, vm: Option<&str>) -> Result<String> {
    ensure!(
        model::is_valid_slug(client),
        "invalid client slug: {client}"
    );

    if let Some(vm) = vm {
        ensure!(model::is_valid_slug(vm), "invalid VM name: {vm}");
        Ok(vm.to_owned())
    } else {
        Ok(model::vm_default(client))
    }
}

fn github_key_name(client: &str) -> String {
    format!("{client}_github")
}

fn ensure_host_ssh_include() -> Result<()> {
    let home = home_dir()?;
    let ssh_dir = home.join(".ssh");
    let config = ssh_dir.join("config");
    let include_line = format!("Include {}/.lima/*/ssh.config", home.display());

    fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed to create {}", ssh_dir.display()))?;
    set_permissions(&ssh_dir, 0o700)?;

    let mut current = String::new();
    if config.exists() {
        fs::File::open(&config)
            .with_context(|| format!("failed to read {}", config.display()))?
            .read_to_string(&mut current)
            .with_context(|| format!("failed to read {}", config.display()))?;
    }

    if !current.lines().any(|line| line == include_line) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config)
            .with_context(|| format!("failed to open {}", config.display()))?;
        writeln!(file)?;
        writeln!(file, "{include_line}")?;
    }

    set_permissions(&config, 0o600)?;
    Ok(())
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .context("HOME is not set")
}

fn set_permissions(path: &PathBuf, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?
            .permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }

    #[cfg(not(unix))]
    let _ = (path, mode);

    Ok(())
}

fn vm_exists(vm: &str) -> Result<bool> {
    let output = process::capture({
        let mut command = Command::new("limactl");
        command.arg("list").arg("--format").arg("{{.Name}}");
        command
    })?;

    Ok(output.lines().any(|line| line == vm))
}

fn start_vm(vm: &str, vm_type: &str, cpus: u16, memory: u16, disk: u16) -> Result<()> {
    if vm_exists(vm)? {
        return process::run({
            let mut command = Command::new("limactl");
            command.arg("start").arg(vm);
            command
        });
    }

    process::run({
        let mut command = Command::new("limactl");
        command
            .arg("start")
            .arg("--yes")
            .arg(format!("--name={vm}"))
            .arg(format!("--cpus={cpus}"))
            .arg(format!("--memory={memory}"))
            .arg(format!("--disk={disk}"))
            .arg(format!("--vm-type={vm_type}"))
            .arg("--mount-none")
            .arg("template:docker");
        command
    })
}

fn ensure_vm_type_prerequisites(vm_type: &str) -> Result<()> {
    if vm_type != "qemu" {
        return Ok(());
    }

    let binary = qemu_binary_for_host();
    if process::command_exists(binary) {
        return Ok(());
    }

    bail!(
        "QEMU VM backend requested but `{binary}` is not on PATH.\n\nInstall QEMU with:\n  brew install qemu\n\nIf QEMU is already installed, make sure its bin directory is on PATH.\nFor a VZ-backed VM without setup snapshots, use:\n  --vm-type vz --no-snapshots"
    )
}

fn qemu_binary_for_host() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "qemu-system-aarch64",
        "x86_64" => "qemu-system-x86_64",
        _ => "qemu-system-aarch64",
    }
}

fn wait_for_ssh(host: &str) -> Result<()> {
    for _ in 0..90 {
        let status = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(host)
            .arg("true")
            .status();

        if matches!(status, Ok(status) if status.success()) {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(2));
    }

    bail!("timed out waiting for SSH host {host}")
}

fn setup_vm_github_key(host: &str, client: &str, key_name: &str, comment: &str) -> Result<()> {
    process::ssh_script(
        host,
        &[client, key_name, comment],
        r##"set -euo pipefail

client="$1"
key_name="$2"
comment="$3"
key_path="$HOME/.ssh/${key_name}"
marker_start="# trusted-devcontainers:${client}:github:start"
marker_end="# trusted-devcontainers:${client}:github:end"

mkdir -p "$HOME/.ssh"
chmod 700 "$HOME/.ssh"
touch "$HOME/.ssh/known_hosts"
chmod 600 "$HOME/.ssh/known_hosts"

cat "$HOME/trusted-devcontainers/ssh/github_known_hosts" | while IFS= read -r known_host; do
  grep -qxF "${known_host}" "$HOME/.ssh/known_hosts" || printf '%s\n' "${known_host}" >> "$HOME/.ssh/known_hosts"
done

if [[ ! -f "${key_path}" ]]; then
  ssh-keygen -t ed25519 -N "" -f "${key_path}" -C "${comment}"
fi

touch "$HOME/.ssh/config"
chmod 600 "$HOME/.ssh/config"

if ! grep -qxF "${marker_start}" "$HOME/.ssh/config"; then
  cat >> "$HOME/.ssh/config" <<EOF

${marker_start}
Host github.com
  HostName github.com
  User git
  IdentityFile ~/.ssh/${key_name}
  IdentitiesOnly yes
${marker_end}
EOF
fi

chmod 600 "$HOME/.ssh/config" "${key_path}"
chmod 644 "${key_path}.pub"
"##,
    )
}

fn remove_vm_github_key(host: &str, client: &str, key_name: &str) -> Result<()> {
    process::ssh_script(
        host,
        &[client, key_name],
        r##"set -euo pipefail

client="$1"
key_name="$2"
key_path="$HOME/.ssh/${key_name}"
config="$HOME/.ssh/config"
marker_start="# trusted-devcontainers:${client}:github:start"
marker_end="# trusted-devcontainers:${client}:github:end"

rm -f "${key_path}" "${key_path}.pub"

if [[ -f "${config}" ]]; then
  tmp="$(mktemp)"
  awk -v start="${marker_start}" -v end="${marker_end}" '
    $0 == start { skip = 1; next }
    $0 == end { skip = 0; next }
    !skip { print }
  ' "${config}" > "${tmp}"
  cat "${tmp}" > "${config}"
  rm -f "${tmp}"
  chmod 600 "${config}"
fi

echo "Removed VM-local GitHub key: ~/.ssh/${key_name}"
"##,
    )
}

fn print_key_and_maybe_wait(host: &str, key_name: &str, prompt: bool) -> Result<()> {
    println!();
    println!("Add this public key to your personal GitHub account:");
    println!("GitHub -> Settings -> SSH and GPG keys -> New SSH key");
    println!();

    let public_key = process::capture({
        let mut command = Command::new("ssh");
        command.arg(host).arg(format!("cat ~/.ssh/{key_name}.pub"));
        command
    })?;
    println!("{}", public_key.trim_end());
    println!();

    if prompt {
        print!("Press Enter after the key has been added to GitHub...");
        io::stdout().flush().context("failed to flush stdout")?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read confirmation")?;
    }

    Ok(())
}

fn clone_repo_in_vm(host: &str, repo: &str, clone_url: &str) -> Result<()> {
    process::ssh_script(
        host,
        &[repo, clone_url],
        r#"set -euo pipefail

repo="$1"
clone_url="$2"
repo_dir="$HOME/work/${repo}"

mkdir -p "$(dirname "${repo_dir}")"

if [[ -d "${repo_dir}/.git" ]]; then
  echo "Repo already cloned: ${repo_dir}"
else
  git clone "${clone_url}" "${repo_dir}"
fi

git -C "${repo_dir}" status --short
"#,
    )
}

fn build_images_on_vm(
    host: &str,
    profile: &str,
    namespace: &str,
    version: Option<&str>,
) -> Result<()> {
    let version = version.unwrap_or("");
    process::ssh_script(
        host,
        &[profile, namespace, version],
        r#"set -euo pipefail

profile="$1"
namespace="$2"
version="${3:-}"

cd "$HOME/trusted-devcontainers"

args=(bin/build-images "${profile}" --namespace "${namespace}")
if [[ -n "${version}" ]]; then
  args+=(--version "${version}")
fi

"${args[@]}"
"#,
    )
}

fn apply_devcontainer_in_vm(host: &str, repo: &str, profile: &Profile) -> Result<()> {
    process::ssh_script(
        host,
        &[repo, profile.as_str()],
        r#"set -euo pipefail

repo="$1"
profile="$2"
repo_dir="$HOME/work/${repo}"

cd "${repo_dir}"
"$HOME/trusted-devcontainers/bin/devcontainer-use" "${profile}" .
"#,
    )
}

fn delete_repo_in_vm(host: &str, repo: &str, force: bool) -> Result<()> {
    let force = if force { "1" } else { "0" };
    process::ssh_script(
        host,
        &[repo, force],
        r#"set -euo pipefail

repo="$1"
force="$2"
repo_dir="$HOME/work/${repo}"

if [[ ! -e "${repo_dir}" ]]; then
  echo "Repo checkout not found: ${repo_dir}"
  exit 0
fi

if [[ -d "${repo_dir}/.git" && "${force}" != "1" ]]; then
  status="$(git -C "${repo_dir}" status --porcelain)"
  if [[ -n "${status}" ]]; then
    echo "Refusing to delete dirty checkout: ${repo_dir}" >&2
    echo "Review or commit changes first, or rerun with --force." >&2
    git -C "${repo_dir}" status --short >&2
    exit 1
  fi

  if upstream="$(git -C "${repo_dir}" rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null)"; then
    counts="$(git -C "${repo_dir}" rev-list --left-right --count "${upstream}...HEAD")"
    ahead="${counts##* }"
    if [[ "${ahead}" != "0" ]]; then
      echo "Refusing to delete checkout with unpushed commits: ${repo_dir}" >&2
      echo "Push or export commits first, or rerun with --force." >&2
      exit 1
    fi
  else
    echo "Refusing to delete checkout without an upstream branch: ${repo_dir}" >&2
    echo "Verify commits are no longer needed, then rerun with --force." >&2
    exit 1
  fi
fi

rm -rf "${repo_dir}"
rmdir "$HOME/work" 2>/dev/null || true
echo "Deleted repo checkout: ${repo_dir}"
"#,
    )
}

fn snapshot_vm(vm: &str, tag: &str) -> Result<()> {
    process::run({
        let mut command = Command::new("limactl");
        command.arg("stop").arg(vm);
        command
    })?;

    let create_result = process::capture({
        let mut command = Command::new("limactl");
        command
            .arg("snapshot")
            .arg("create")
            .arg(vm)
            .arg("--tag")
            .arg(tag);
        command
    });

    let start_result = process::run({
        let mut command = Command::new("limactl");
        command.arg("start").arg(vm);
        command
    });

    start_result?;
    create_result.with_context(|| {
        format!(
            "failed to create snapshot '{tag}'. If this VM uses Lima's vz backend, snapshots may be unavailable; retry with --vm-type qemu for snapshot support or --no-snapshots to skip setup snapshots"
        )
    })?;

    wait_for_ssh(&model::lima_host(vm))
}

fn apply_snapshot(vm: &str, tag: &str) -> Result<()> {
    process::run({
        let mut command = Command::new("limactl");
        command.arg("stop").arg(vm);
        command
    })?;

    let apply_result = process::capture({
        let mut command = Command::new("limactl");
        command
            .arg("snapshot")
            .arg("apply")
            .arg(vm)
            .arg("--tag")
            .arg(tag);
        command
    });

    let start_result = process::run({
        let mut command = Command::new("limactl");
        command.arg("start").arg(vm);
        command
    });

    start_result?;
    apply_result.with_context(|| {
        format!(
            "failed to apply snapshot '{tag}'. If this VM uses Lima's vz backend, snapshots may be unavailable; use --vm-type qemu for snapshot support"
        )
    })?;

    wait_for_ssh(&model::lima_host(vm))
}

fn print_next_steps(
    client: &str,
    repo: &RepoSpec,
    vm: &str,
    host: &str,
    profile: &Profile,
    skip_build: bool,
) {
    println!();
    println!("VM ready: {vm}");
    println!("SSH host: {host}");
    println!("Repo path: ~/work/{}", repo.repo);
    println!("Clone URL: {}", repo.clone_url);
    println!("Profile: {}", profile.as_str());
    println!();
    println!("Next:");
    if skip_build {
        println!(
            "  1. Build the profile image: tdc images build {} --client {client}",
            profile.as_str()
        );
        println!("  2. VS Code: Remote-SSH: Connect to Host -> {host}");
        println!("  3. Open folder: ~/work/{}", repo.repo);
        println!("  4. Command Palette: Dev Containers: Reopen in Container");
    } else {
        println!("  1. VS Code: Remote-SSH: Connect to Host -> {host}");
        println!("  2. Open folder: ~/work/{}", repo.repo);
        println!("  3. Command Palette: Dev Containers: Reopen in Container");
    }
}
