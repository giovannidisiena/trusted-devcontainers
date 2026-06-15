use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use include_dir::{Dir, DirEntry, include_dir};
use tempfile::{Builder, TempDir};

use crate::process;

static PAYLOAD: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets/payload");

pub fn packaged_version() -> Result<String> {
    let version = PAYLOAD
        .get_file("VERSION")
        .context("embedded payload is missing VERSION")?;

    Ok(String::from_utf8_lossy(version.contents())
        .trim()
        .to_owned())
}

pub fn materialize() -> Result<TempDir> {
    let temp = Builder::new()
        .prefix("tdc-payload-")
        .tempdir()
        .context("failed to create payload staging directory")?;

    write_dir(&PAYLOAD, temp.path())?;
    Ok(temp)
}

pub fn sync_to_vm(host: &str) -> Result<()> {
    let temp = materialize()?;
    process::ssh_script(
        host,
        &[],
        r#"set -euo pipefail
mkdir -p "$HOME/trusted-devcontainers"
"#,
    )?;

    let source = format!("{}/", temp.path().display());
    let dest = format!("{host}:~/trusted-devcontainers/");
    process::run({
        let mut command = Command::new("rsync");
        command.arg("-a").arg("--delete").arg(source).arg(dest);
        command
    })?;

    process::ssh_script(
        host,
        &[],
        r#"set -euo pipefail
chmod +x "$HOME/trusted-devcontainers/bin/"*
chmod +x "$HOME/trusted-devcontainers/images/scripts/"*.sh
"#,
    )?;

    Ok(())
}

fn write_dir(dir: &Dir<'_>, base: &Path) -> Result<()> {
    for entry in dir.entries() {
        write_entry(entry, base)?;
    }

    Ok(())
}

fn write_entry(entry: &DirEntry<'_>, base: &Path) -> Result<()> {
    match entry {
        DirEntry::Dir(dir) => {
            let target = base.join(dir.path());
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
            write_dir(dir, base)
        }
        DirEntry::File(file) => {
            let target = base.join(file.path());
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }

            let mut output = fs::File::create(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
            output
                .write_all(file.contents())
                .with_context(|| format!("failed to write {}", target.display()))?;

            make_executable_if_needed(&target, file.path())?;
            Ok(())
        }
    }
}

fn make_executable_if_needed(target: &Path, payload_path: &Path) -> Result<()> {
    if !is_executable_payload(payload_path) {
        return Ok(());
    }

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(target)
            .with_context(|| format!("failed to stat {}", target.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(target, permissions)
            .with_context(|| format!("failed to chmod {}", target.display()))?;
    }

    Ok(())
}

fn is_executable_payload(path: &Path) -> bool {
    path.starts_with("bin") || path.starts_with("images/scripts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_expected_payload_files() {
        assert!(PAYLOAD.get_file("VERSION").is_some());
        assert!(PAYLOAD.get_file("bin/build-images").is_some());
        assert!(PAYLOAD.get_file("bin/devcontainer-use").is_some());
        assert!(PAYLOAD.get_file("ssh/github_known_hosts").is_some());
        assert!(PAYLOAD.get_file("images/base/Dockerfile").is_some());
        assert!(
            PAYLOAD
                .get_file("configs/solidity-foundry/.devcontainer/devcontainer.json")
                .is_some()
        );
    }

    #[test]
    fn materializes_payload() {
        let temp = materialize().unwrap();
        assert!(temp.path().join("VERSION").is_file());
        assert!(temp.path().join("bin/build-images").is_file());
        assert!(temp.path().join("images/base/Dockerfile").is_file());
    }
}
