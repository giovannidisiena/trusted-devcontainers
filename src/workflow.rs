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
    if shell == clap_complete::Shell::Zsh {
        return zsh_completion();
    }

    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "tdc", &mut io::stdout());
    Ok(())
}

fn zsh_completion() -> Result<()> {
    let script = zsh_completion_script()?;
    io::stdout()
        .write_all(script.as_bytes())
        .context("failed to write zsh completion")?;
    Ok(())
}

fn zsh_completion_script() -> Result<String> {
    let mut command = Cli::command();
    let mut output = Vec::new();
    clap_complete::generate(clap_complete::Shell::Zsh, &mut command, "tdc", &mut output);

    let mut script = String::from_utf8(output).context("failed to render zsh completion")?;
    script = script.replace(":CLIENT:_default", ":CLIENT:_tdc_complete_clients");
    script = script.replace(":VM:_default", ":VM:_tdc_complete_vms");
    script = script.replace(":TAG:_default", ":TAG:_tdc_complete_snapshot_tags");
    script.push_str(ZSH_DYNAMIC_COMPLETIONS);
    Ok(script)
}

const ZSH_DYNAMIC_COMPLETIONS: &str = r#"

_tdc_complete_clients() {
    local -a clients
    clients=("${(@f)$(_call_program tdc-clients tdc __complete clients 2>/dev/null)}")
    if (( ${#clients[@]} )); then
        compadd -a clients
    else
        _message 'no tdc client VMs found'
    fi
}

_tdc_complete_vms() {
    local -a vms
    vms=("${(@f)$(_call_program tdc-vms tdc __complete vms 2>/dev/null)}")
    if (( ${#vms[@]} )); then
        compadd -a vms
    else
        _message 'no Lima VMs found'
    fi
}

_tdc_complete_snapshot_tags() {
    local -a args tags
    local client vm word
    local i

    for (( i = 1; i <= ${#words[@]}; i++ )); do
        word="${words[i]}"
        case "${word}" in
            --client)
                if (( i < ${#words[@]} )); then
                    client="${words[i + 1]}"
                fi
                ;;
            --client=*)
                client="${word#--client=}"
                ;;
            --vm)
                if (( i < ${#words[@]} )); then
                    vm="${words[i + 1]}"
                fi
                ;;
            --vm=*)
                vm="${word#--vm=}"
                ;;
        esac
    done

    if [[ -n "${client}" ]]; then
        args+=(--client "${client}")
    fi
    if [[ -n "${vm}" ]]; then
        args+=(--vm "${vm}")
    fi

    tags=("${(@f)$(_call_program tdc-snapshot-tags tdc __complete snapshot-tags "${args[@]}" 2>/dev/null)}")
    if (( ${#tags[@]} )); then
        compadd -a tags
    else
        _message 'no snapshots found'
    fi
}
"#;

pub fn complete<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("clients") => complete_clients(),
        Some("vms") => complete_vms(),
        Some("snapshot-tags") => complete_snapshot_tags(parse_completion_target(args)),
        _ => Ok(()),
    }
}

fn complete_clients() -> Result<()> {
    let clients = lima_vm_names()
        .into_iter()
        .filter_map(|vm| client_from_vm_name(&vm).map(str::to_owned))
        .collect::<Vec<_>>();

    print_completion_lines(clients)
}

fn complete_vms() -> Result<()> {
    print_completion_lines(lima_vm_names())
}

fn complete_snapshot_tags(args: VmTargetArgs) -> Result<()> {
    let Some(vm) = completion_target_vm(&args) else {
        return Ok(());
    };

    print_completion_lines(snapshot_tags(&vm))
}

fn parse_completion_target<I>(args: I) -> VmTargetArgs
where
    I: IntoIterator<Item = String>,
{
    let mut client = None;
    let mut vm = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--client" => client = args.next(),
            "--vm" => vm = args.next(),
            _ => {
                if let Some(value) = arg.strip_prefix("--client=") {
                    client = Some(value.to_owned());
                } else if let Some(value) = arg.strip_prefix("--vm=") {
                    vm = Some(value.to_owned());
                }
            }
        }
    }

    VmTargetArgs { client, vm }
}

fn print_completion_lines(mut values: Vec<String>) -> Result<()> {
    values.sort();
    values.dedup();
    for value in values {
        println!("{value}");
    }
    Ok(())
}

fn lima_vm_names() -> Vec<String> {
    if !process::command_exists("limactl") {
        return Vec::new();
    }

    let Ok(output) = Command::new("limactl")
        .arg("list")
        .arg("--quiet")
        .arg("--tty=false")
        .output()
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| model::is_valid_slug(line))
        .map(str::to_owned)
        .collect()
}

fn snapshot_tags(vm: &str) -> Vec<String> {
    if !process::command_exists("limactl") {
        return Vec::new();
    }

    let Ok(output) = Command::new("limactl")
        .arg("snapshot")
        .arg("list")
        .arg(vm)
        .arg("--quiet")
        .arg("--tty=false")
        .output()
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn client_from_vm_name(vm: &str) -> Option<&str> {
    let client = vm.strip_prefix("client-")?;
    if model::is_valid_slug(client) {
        Some(client)
    } else {
        None
    }
}

fn completion_target_vm(args: &VmTargetArgs) -> Option<String> {
    match (&args.client, &args.vm) {
        (Some(_), Some(_)) | (None, None) => None,
        (Some(client), None) if model::is_valid_slug(client) => Some(model::vm_default(client)),
        (None, Some(vm)) if model::is_valid_slug(vm) => Some(vm.clone()),
        _ => None,
    }
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
    ensure_vm_user_local_bin_path(&host)?;

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
        ensure_profile_image_available(&host, &args.profile, &args.client, &vm)?;
    }

    if !args.skip_clone {
        if args.skip_build {
            print_skip_build_image_warning(&args.client, &vm, &args.profile);
        }
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
    let host = model::lima_host(&vm);
    ensure_vm_user_local_bin_path(&host)?;
    process::run({
        let mut command = Command::new("ssh");
        command.arg(host);
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
    print_trusted_image_status_if_available(&vm)?;

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
    let profile = args.profile.as_str();

    ensure_host_ssh_include()?;
    ensure_vm_user_local_bin_path(&host)?;
    payload::sync_to_vm(&host)?;
    build_images_on_vm(&host, profile, &args.namespace, args.version.as_deref())?;
    ensure_built_images_available(&host, profile, &args.namespace, args.version.as_deref())
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
    ensure_vm_user_local_bin_path(&host)?;
    payload::sync_to_vm(&host)?;
    ensure_profile_image_available(&host, &args.profile, &args.client, &vm)?;
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

fn ensure_vm_user_local_bin_path(host: &str) -> Result<()> {
    process::ssh_script(
        host,
        &[],
        r##"set -euo pipefail

mkdir -p "$HOME/.local/bin"

for shell_file in "$HOME/.profile" "$HOME/.bashrc"; do
  touch "${shell_file}"
  if grep -qxF "# trusted-devcontainers:user-path:start" "${shell_file}"; then
    continue
  fi

  cat >> "${shell_file}" <<'EOF'

# trusted-devcontainers:user-path:start
if [ -d "$HOME/.local/bin" ]; then
  case ":$PATH:" in
    *:"$HOME/.local/bin":*) ;;
    *) export PATH="$HOME/.local/bin:$PATH" ;;
  esac
fi
# trusted-devcontainers:user-path:end
EOF
done
"##,
    )
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

fn lima_vm_status(vm: &str) -> Result<Option<String>> {
    let output = process::capture({
        let mut command = Command::new("limactl");
        command
            .arg("list")
            .arg("--format")
            .arg("{{.Name}}\t{{.Status}}");
        command
    })?;

    for line in output.lines() {
        let Some((name, status)) = line.split_once('\t') else {
            continue;
        };
        if name == vm {
            return Ok(Some(status.trim().to_owned()));
        }
    }

    Ok(None)
}

fn vm_exists(vm: &str) -> Result<bool> {
    Ok(lima_vm_status(vm)?.is_some())
}

fn vm_is_running(vm: &str) -> Result<bool> {
    Ok(lima_vm_status(vm)?
        .as_deref()
        .is_some_and(|status| status.eq_ignore_ascii_case("running")))
}

fn start_vm(vm: &str, vm_type: &str, cpus: u16, memory: u16, disk: u16) -> Result<()> {
    if vm_exists(vm)? {
        if vm_is_running(vm)? {
            return Ok(());
        }

        return process::run({
            let mut command = Command::new("limactl");
            command.arg("start").arg(vm);
            command
        });
    }

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

    let display = process::display_command(&command);
    let status = command
        .status()
        .with_context(|| format!("failed to start {display}"))?;

    if status.success() {
        return Ok(());
    }

    if vm_is_running(vm)? {
        eprintln!(
            "warning: {display} exited with {status}, but VM '{vm}' is running; continuing setup"
        );
        return Ok(());
    }

    bail!("{display} exited with {status}")
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

fn ensure_profile_image_available(
    host: &str,
    profile: &Profile,
    client: &str,
    vm: &str,
) -> Result<()> {
    let version = payload::packaged_version()?;
    let image_ref = trusted_image_ref(profile.as_str(), &version);
    if image_exists_in_vm(host, &image_ref)? {
        return Ok(());
    }

    let target = build_target_selector(client, vm);
    bail!(
        "missing trusted image in VM: {image_ref}\n\nBuild it with:\n  tdc images build {} {target}\n\nThen retry the devcontainer operation.",
        profile.as_str()
    )
}

fn ensure_built_images_available(
    host: &str,
    profile: &str,
    namespace: &str,
    version: Option<&str>,
) -> Result<()> {
    let version = version_or_packaged(version)?;
    let mut missing = Vec::new();

    for image_ref in image_refs_for_build_target(profile, namespace, &version) {
        if !image_exists_in_vm(host, &image_ref)? {
            missing.push(image_ref);
        }
    }

    if missing.is_empty() {
        return Ok(());
    }

    bail!(
        "image build completed, but expected image(s) are missing in the VM:\n  {}",
        missing.join("\n  ")
    )
}

fn image_exists_in_vm(host: &str, image_ref: &str) -> Result<bool> {
    let output = Command::new("ssh")
        .arg(host)
        .arg("docker")
        .arg("image")
        .arg("inspect")
        .arg(image_ref)
        .arg("--format")
        .arg("{{.Id}}")
        .output()
        .with_context(|| format!("failed to inspect image {image_ref} on {host}"))?;

    Ok(output.status.success())
}

fn print_trusted_image_status_if_available(vm: &str) -> Result<()> {
    if !process::command_exists("ssh") {
        eprintln!("Trusted images: unavailable; ssh is not on PATH");
        return Ok(());
    }

    let version = payload::packaged_version()?;
    let image_refs = image_refs_for_build_target("all", "trusted", &version);
    let mut args = vec![version];
    args.extend(image_refs);
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let host = model::lima_host(vm);

    if let Err(err) = process::ssh_script(
        &host,
        &args,
        r#"set -euo pipefail

version="$1"
shift

echo "Trusted images (${version}):"
for image_ref in "$@"; do
  if docker image inspect "${image_ref}" >/dev/null 2>&1; then
    echo "  ok: ${image_ref}"
  else
    echo "  missing: ${image_ref}"
  fi
done
"#,
    ) {
        eprintln!("Trusted images: unavailable ({err:#})");
    }

    Ok(())
}

fn print_skip_build_image_warning(client: &str, vm: &str, profile: &Profile) {
    let target = build_target_selector(client, vm);
    eprintln!(
        "warning: --skip-build leaves the devcontainer config pointing at a local image that may not exist"
    );
    eprintln!(
        "warning: build it before opening VS Code: tdc images build {} {target}",
        profile.as_str()
    );
}

fn version_or_packaged(version: Option<&str>) -> Result<String> {
    match version {
        Some(version) if !version.is_empty() => Ok(version.to_owned()),
        _ => payload::packaged_version(),
    }
}

fn trusted_image_ref(image: &str, version: &str) -> String {
    image_ref("trusted", image, version)
}

fn image_ref(namespace: &str, image: &str, version: &str) -> String {
    format!("{namespace}/{image}:{version}")
}

fn image_refs_for_build_target(profile: &str, namespace: &str, version: &str) -> Vec<String> {
    match profile {
        "all" => ["base", "node", "solidity-foundry", "solidity-foundry-node"]
            .into_iter()
            .map(|image| image_ref(namespace, image, version))
            .collect(),
        image => vec![image_ref(namespace, image, version)],
    }
}

fn build_target_selector(client: &str, vm: &str) -> String {
    if vm == model::vm_default(client) {
        format!("--client {client}")
    } else {
        format!("--vm {vm}")
    }
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
        let target = build_target_selector(client, vm);
        println!(
            "  1. Build the profile image: tdc images build {} {target}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_client_prefix_for_client_completion() {
        assert_eq!(client_from_vm_name("client-polymarket"), Some("polymarket"));
        assert_eq!(
            client_from_vm_name("client-client-polymarket"),
            Some("client-polymarket")
        );
        assert_eq!(client_from_vm_name("polymarket"), None);
    }

    #[test]
    fn parses_completion_target_flags() {
        let target = parse_completion_target([
            "--client".to_owned(),
            "polymarket".to_owned(),
            "--ignored".to_owned(),
        ]);
        assert_eq!(target.client.as_deref(), Some("polymarket"));
        assert_eq!(target.vm.as_deref(), None);

        let target = parse_completion_target(["--vm=client-polymarket".to_owned()]);
        assert_eq!(target.client.as_deref(), None);
        assert_eq!(target.vm.as_deref(), Some("client-polymarket"));
    }

    #[test]
    fn zsh_completion_uses_dynamic_vm_completers() {
        let script = zsh_completion_script().unwrap();
        assert!(script.contains(":CLIENT:_tdc_complete_clients"));
        assert!(script.contains(":VM:_tdc_complete_vms"));
        assert!(script.contains(":TAG:_tdc_complete_snapshot_tags"));
        assert!(script.contains("tdc __complete clients"));
        assert!(!script.contains(":CLIENT:_default"));
        assert!(!script.contains(":VM:_default"));
        assert!(!script.contains("'__complete:"));
        assert!(!script.contains("\n(__complete)"));
    }

    #[test]
    fn renders_image_refs_for_single_and_all_build_targets() {
        assert_eq!(
            image_refs_for_build_target("node", "trusted", "0.1.1"),
            vec!["trusted/node:0.1.1"]
        );

        assert_eq!(
            image_refs_for_build_target("all", "trusted", "0.1.1"),
            vec![
                "trusted/base:0.1.1",
                "trusted/node:0.1.1",
                "trusted/solidity-foundry:0.1.1",
                "trusted/solidity-foundry-node:0.1.1",
            ]
        );
    }

    #[test]
    fn selects_build_target_flag_for_default_and_custom_vm_names() {
        assert_eq!(
            build_target_selector("polymarket", "client-polymarket"),
            "--client polymarket"
        );
        assert_eq!(
            build_target_selector("polymarket", "audit-polymarket"),
            "--vm audit-polymarket"
        );
    }
}
