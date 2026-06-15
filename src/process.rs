use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub fn require_commands(commands: &[&str]) -> Result<()> {
    for command in commands {
        if !command_exists(command) {
            bail!("missing required command on PATH: {command}");
        }
    }

    Ok(())
}

pub fn command_exists(command: &str) -> bool {
    if command.contains('/') {
        return is_executable(Path::new(command));
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|path| is_executable(&path.join(command)))
}

pub fn run(mut command: Command) -> Result<()> {
    let display = display_command(&command);
    let status = command
        .status()
        .with_context(|| format!("failed to start {display}"))?;

    if !status.success() {
        bail!("{display} exited with {status}");
    }

    Ok(())
}

pub fn capture(mut command: Command) -> Result<String> {
    let display = display_command(&command);
    let output = command
        .output()
        .with_context(|| format!("failed to start {display}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{display} exited with {}: {}", output.status, stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn ssh_script(host: &str, args: &[&str], script: &str) -> Result<()> {
    let mut command = Command::new("ssh");
    command.arg(host).arg("bash").arg("-s").arg("--").args(args);
    let display = display_command(&command);

    let mut child = command
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {display}"))?;

    child
        .stdin
        .take()
        .context("failed to open ssh stdin")?
        .write_all(script.as_bytes())
        .context("failed to write ssh script")?;

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for {display}"))?;

    if !status.success() {
        bail!("{display} exited with {status}");
    }

    Ok(())
}

pub fn display_command(command: &Command) -> String {
    let mut parts = Vec::new();
    parts.push(shell_display(command.get_program()));
    parts.extend(command.get_args().map(shell_display));
    parts.join(" ")
}

fn shell_display(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_' | b':' | b'@'))
    {
        value.into_owned()
    } else {
        format!("{value:?}")
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
