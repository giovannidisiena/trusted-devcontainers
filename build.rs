use std::fs;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    emit_rerun_if_changed(Path::new("payload"))
}

fn emit_rerun_if_changed(path: &Path) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", path.display());

    if !path.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(path)? {
        emit_rerun_if_changed(&entry?.path())?;
    }

    Ok(())
}
