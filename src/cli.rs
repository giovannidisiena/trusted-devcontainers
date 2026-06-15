use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::model::{
    BuildTarget, DEFAULT_CPUS, DEFAULT_DISK_GB, DEFAULT_MEMORY_GB, DEFAULT_ORG, Profile, VmType,
};

#[derive(Debug, Parser)]
#[command(
    name = "tdc",
    bin_name = "tdc",
    version,
    about = "Provision trusted Lima devcontainer workspaces",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Manage client Lima VMs")]
    Vm(VmArgs),
    #[command(about = "Manage repository checkouts inside VMs")]
    Repo(RepoArgs),
    #[command(about = "Build trusted container images inside a VM")]
    Images(ImagesArgs),
    #[command(about = "Manage devcontainer configs inside a VM")]
    Devcontainer(DevcontainerArgs),
    #[command(about = "Print shell completion scripts")]
    Completion(CompletionArgs),
    #[command(about = "Show or print a tdc(1) manpage")]
    Manpage(ManpageArgs),
    #[command(about = "Check host prerequisites")]
    Doctor,
}

#[derive(Debug, Args)]
pub struct VmArgs {
    #[command(subcommand)]
    pub command: VmCommand,
}

#[derive(Debug, Subcommand)]
pub enum VmCommand {
    #[command(
        about = "Create or reuse a client VM and prepare a repository",
        override_usage = "tdc vm new [OPTIONS] --client <CLIENT> [--repo <REPO>|--repo-url <URL>]"
    )]
    New(VmNewArgs),
    #[command(about = "List Lima VMs")]
    List,
    #[command(
        about = "Manage Lima snapshots",
        override_usage = "tdc vm snapshot <COMMAND>"
    )]
    Snapshot(VmSnapshotArgs),
    #[command(about = "Manage VM-local GitHub keys")]
    Key(VmKeyArgs),
    #[command(
        about = "Open an interactive SSH session",
        override_usage = "tdc vm ssh [--client <CLIENT>|--vm <VM>]"
    )]
    Ssh(VmTargetArgs),
    #[command(
        about = "Show VM and snapshot status",
        override_usage = "tdc vm status [--client <CLIENT>|--vm <VM>]"
    )]
    Status(VmTargetArgs),
    #[command(
        about = "Start a client VM",
        override_usage = "tdc vm start [--client <CLIENT>|--vm <VM>]"
    )]
    Start(VmTargetArgs),
    #[command(
        about = "Stop a client VM",
        override_usage = "tdc vm stop [--client <CLIENT>|--vm <VM>]"
    )]
    Stop(VmStopArgs),
    #[command(
        about = "Delete a client VM",
        override_usage = "tdc vm delete [--client <CLIENT>|--vm <VM>]"
    )]
    Delete(VmDeleteArgs),
}

#[derive(Debug, Args)]
pub struct RepoInputArgs {
    #[arg(long, help = "GitHub repo name, combined with --org")]
    pub repo: Option<String>,
    #[arg(long = "repo-url", help = "GitHub SSH or HTTPS repository URL")]
    pub repo_url: Option<String>,
    #[arg(long, default_value = DEFAULT_ORG, help = "GitHub owner or organization for --repo")]
    pub org: String,
}

#[derive(Debug, Args)]
pub struct VmNewArgs {
    #[arg(long, help = "Local client/workspace slug")]
    pub client: String,
    #[command(flatten)]
    pub repo: RepoInputArgs,
    #[arg(
        long,
        value_enum,
        default_value = "solidity-foundry",
        help = "Trusted devcontainer profile"
    )]
    pub profile: Profile,
    #[arg(long, help = "Override VM name")]
    pub vm: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "vz",
        help = "Lima VM backend. VM type cannot be changed after creation"
    )]
    pub vm_type: VmType,
    #[arg(long, default_value_t = DEFAULT_CPUS, help = "CPU count")]
    pub cpus: u16,
    #[arg(long, default_value_t = DEFAULT_MEMORY_GB, help = "Memory in GB")]
    pub memory: u16,
    #[arg(long, default_value_t = DEFAULT_DISK_GB, help = "Disk in GB")]
    pub disk: u16,
    #[arg(long, help = "Do not wait after printing the public key")]
    pub no_prompt: bool,
    #[arg(long, help = "Do not clone the GitHub repo")]
    pub skip_clone: bool,
    #[arg(long, help = "Do not build container images")]
    pub skip_build: bool,
    #[arg(long, help = "Do not create Lima snapshots during setup")]
    pub no_snapshots: bool,
}

#[derive(Debug, Args)]
pub struct VmSnapshotArgs {
    #[command(subcommand)]
    pub command: VmSnapshotCommand,
}

#[derive(Debug, Subcommand)]
pub enum VmSnapshotCommand {
    #[command(
        about = "List Lima snapshots",
        override_usage = "tdc vm snapshot list [--client <CLIENT>|--vm <VM>]"
    )]
    List(VmTargetArgs),
    #[command(
        about = "Create a Lima snapshot",
        override_usage = "tdc vm snapshot create --tag <TAG> [--client <CLIENT>|--vm <VM>]"
    )]
    Create(VmSnapshotTagArgs),
    #[command(
        about = "Apply a Lima snapshot",
        override_usage = "tdc vm snapshot apply --tag <TAG> [--client <CLIENT>|--vm <VM>]"
    )]
    Apply(VmSnapshotTagArgs),
    #[command(
        about = "Delete a Lima snapshot",
        override_usage = "tdc vm snapshot delete --tag <TAG> [--client <CLIENT>|--vm <VM>]"
    )]
    Delete(VmSnapshotTagArgs),
}

#[derive(Debug, Args)]
pub struct VmSnapshotTagArgs {
    #[command(flatten)]
    pub target: VmTargetArgs,
    #[arg(long, help = "Snapshot tag")]
    pub tag: String,
}

#[derive(Debug, Args)]
pub struct VmKeyArgs {
    #[command(subcommand)]
    pub command: VmKeyCommand,
}

#[derive(Debug, Subcommand)]
pub enum VmKeyCommand {
    #[command(
        about = "Print the VM-local GitHub public key",
        override_usage = "tdc vm key show --client <CLIENT> [--vm <VM>]"
    )]
    Show(VmClientTargetArgs),
    #[command(
        about = "Remove the VM-local GitHub key and SSH config block",
        override_usage = "tdc vm key remove --client <CLIENT> [--vm <VM>] --yes"
    )]
    Remove(VmKeyRemoveArgs),
}

#[derive(Debug, Args)]
pub struct VmClientTargetArgs {
    #[arg(long, help = "Local client/workspace slug")]
    pub client: String,
    #[arg(long, help = "Override VM name")]
    pub vm: Option<String>,
}

#[derive(Debug, Args)]
pub struct VmKeyRemoveArgs {
    #[command(flatten)]
    pub target: VmClientTargetArgs,
    #[arg(long, help = "Confirm removal of the VM-local GitHub key")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct VmStopArgs {
    #[command(flatten)]
    pub target: VmTargetArgs,
    #[arg(long, help = "Force stop the VM")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct VmDeleteArgs {
    #[command(flatten)]
    pub target: VmTargetArgs,
    #[arg(long, help = "Forcibly kill VM processes during delete")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct VmTargetArgs {
    #[arg(long, help = "Local client/workspace slug")]
    pub client: Option<String>,
    #[arg(long, help = "VM name")]
    pub vm: Option<String>,
}

#[derive(Debug, Args)]
pub struct RepoArgs {
    #[command(subcommand)]
    pub command: RepoCommand,
}

#[derive(Debug, Subcommand)]
pub enum RepoCommand {
    #[command(
        about = "Delete a repository checkout from a retained VM",
        override_usage = "tdc repo delete [OPTIONS] --client <CLIENT> [--repo <REPO>|--repo-url <URL>]"
    )]
    Delete(RepoDeleteArgs),
}

#[derive(Debug, Args)]
pub struct RepoDeleteArgs {
    #[arg(long, help = "Local client/workspace slug")]
    pub client: String,
    #[arg(long, help = "Override VM name")]
    pub vm: Option<String>,
    #[command(flatten)]
    pub repo: RepoInputArgs,
    #[arg(long, help = "Delete even when the checkout is dirty or unpushed")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct ImagesArgs {
    #[command(subcommand)]
    pub command: ImagesCommand,
}

#[derive(Debug, Subcommand)]
pub enum ImagesCommand {
    #[command(
        about = "Build one or more trusted images inside a VM",
        override_usage = "tdc images build [OPTIONS] [--client <CLIENT>|--vm <VM>] [PROFILE]"
    )]
    Build(ImagesBuildArgs),
}

#[derive(Debug, Args)]
pub struct ImagesBuildArgs {
    #[arg(value_enum, default_value = "all")]
    pub profile: BuildTarget,
    #[command(flatten)]
    pub target: VmTargetArgs,
    #[arg(long, default_value = "trusted")]
    pub namespace: String,
    #[arg(
        long,
        help = "Image version tag. Defaults to the packaged VERSION file"
    )]
    pub version: Option<String>,
}

#[derive(Debug, Args)]
pub struct DevcontainerArgs {
    #[command(subcommand)]
    pub command: DevcontainerCommand,
}

#[derive(Debug, Subcommand)]
pub enum DevcontainerCommand {
    #[command(
        about = "Apply a devcontainer profile to a cloned repo inside a VM",
        override_usage = "tdc devcontainer use [OPTIONS] --client <CLIENT> --profile <PROFILE> [--repo <REPO>|--repo-url <URL>]"
    )]
    Use(DevcontainerUseArgs),
}

#[derive(Debug, Args)]
pub struct DevcontainerUseArgs {
    #[arg(long, help = "Local client/workspace slug")]
    pub client: String,
    #[arg(long, help = "Override VM name")]
    pub vm: Option<String>,
    #[command(flatten)]
    pub repo: RepoInputArgs,
    #[arg(long, value_enum, help = "Trusted devcontainer profile")]
    pub profile: Profile,
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Args)]
pub struct ManpageArgs {
    #[arg(long, help = "Print raw roff manpage source instead of rendering it")]
    pub raw: bool,
    #[arg(long, help = "Install tdc.1 into a manpath directory")]
    pub install: bool,
    #[arg(
        long = "install-dir",
        value_name = "DIR",
        help = "Manpath base directory for --install"
    )]
    pub install_dir: Option<PathBuf>,
}
