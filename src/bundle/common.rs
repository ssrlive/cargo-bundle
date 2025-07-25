use anyhow::Context;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Component, Path, PathBuf};

/// Returns true if the path has a filename indicating that it is a high-desity
/// "retina" icon.  Specifically, returns true the the file stem ends with
/// "@2x" (a convention specified by the [Apple developer docs](
/// https://developer.apple.com/library/mac/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Optimizing/Optimizing.html)).
pub fn is_retina<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .file_stem()
        .and_then(OsStr::to_str)
        .map(|stem| stem.ends_with("@2x"))
        .unwrap_or(false)
}

/// Creates a new file at the given path, creating any parent directories as
/// needed.
pub fn create_file(path: &Path) -> crate::Result<BufWriter<File>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {parent:?}"))?;
    }
    let file = File::create(path).with_context(|| format!("Failed to create file {path:?}"))?;
    Ok(BufWriter::new(file))
}

#[cfg(unix)]
fn symlink_dir(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
fn symlink_dir(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(src, dst)
}

#[cfg(unix)]
pub fn symlink_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
pub fn symlink_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(src, dst)
}

/// Copies a regular file from one path to another, creating any parent
/// directories of the destination path as necessary.  Fails if the source path
/// is a directory or doesn't exist.
pub fn copy_file(from: &Path, to: &Path) -> crate::Result<()> {
    if !from.exists() {
        anyhow::bail!("{:?} does not exist", from);
    }
    if !from.is_file() {
        anyhow::bail!("{:?} is not a file", from);
    }
    let dest_dir = to.parent().unwrap();
    fs::create_dir_all(dest_dir).with_context(|| format!("Failed to create {dest_dir:?}"))?;
    fs::copy(from, to).with_context(|| format!("Failed to copy {from:?} to {to:?}"))?;
    Ok(())
}

/// Reads a regular file into memory
pub fn read_file(file: &Path) -> crate::Result<String> {
    if !file.exists() {
        anyhow::bail!("{:?} does not exist", file);
    }
    if !file.is_file() {
        anyhow::bail!("{:?} is not a file", file);
    }
    let contents = fs::read_to_string(file).with_context(|| format!("Failed to read {file:?}"))?;
    Ok(contents)
}

/// Recursively copies a directory file from one path to another, creating any
/// parent directories of the destination path as necessary.  Fails if the
/// source path is not a directory or doesn't exist, or if the destination path
/// already exists.
pub fn copy_dir(from: &Path, to: &Path) -> crate::Result<()> {
    if !from.exists() {
        anyhow::bail!("{:?} does not exist", from);
    }
    if !from.is_dir() {
        anyhow::bail!("{:?} is not a directory", from);
    }
    if to.exists() {
        anyhow::bail!("{:?} already exists", to);
    }
    let parent = to.parent().unwrap();
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {parent:?}"))?;
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        debug_assert!(entry.path().starts_with(from));
        let rel_path = entry.path().strip_prefix(from).unwrap();
        let dest_path = to.join(rel_path);
        if entry.file_type().is_symlink() {
            let target = fs::read_link(entry.path())?;
            if entry.path().is_dir() {
                symlink_dir(&target, &dest_path)?;
            } else {
                symlink_file(&target, &dest_path)?;
            }
        } else if entry.file_type().is_dir() {
            fs::create_dir(dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

/// Given a path (absolute or relative) to a resource file, returns the
/// relative path from the bundle resources directory where that resource
/// should be stored.
pub fn resource_relpath(path: &Path) -> PathBuf {
    let mut dest = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => {}
            Component::RootDir => dest.push("_root_"),
            Component::CurDir => {}
            Component::ParentDir => dest.push("_up_"),
            Component::Normal(string) => dest.push(string),
        }
    }
    dest
}

/// Prints a message to stderr, in the same format that `cargo` uses,
/// indicating that we are creating a bundle with the given filename.
pub fn print_bundling(filename: &str) -> crate::Result<()> {
    print_progress("Bundling", filename)
}

/// Prints a message to stderr, in the same format that `cargo` uses,
/// indicating that we have finished the the given bundles.
pub fn print_finished(output_paths: &Vec<PathBuf>) -> crate::Result<()> {
    let pluralised = if output_paths.len() == 1 {
        "bundle"
    } else {
        "bundles"
    };
    let msg = format!("{} {} at:", output_paths.len(), pluralised);
    print_progress("Finished", &msg)?;
    for path in output_paths {
        println!("        {}", path.display());
    }
    Ok(())
}

fn safe_term_attr<T: term::Terminal + ?Sized>(
    output: &mut Box<T>,
    attr: term::Attr,
) -> term::Result<()> {
    match output.supports_attr(attr) {
        true => output.attr(attr),
        false => Ok(()),
    }
}

fn print_progress(step: &str, msg: &str) -> crate::Result<()> {
    if let Some(mut output) = term::stderr() {
        safe_term_attr(&mut output, term::Attr::Bold)?;
        if output.supports_color() {
            output.fg(term::color::GREEN)?;
        }
        write!(output, "    {step}")?;
        if output.supports_reset() {
            output.reset()?;
        }
        writeln!(output, " {msg}")?;
        output.flush()?;
        Ok(())
    } else {
        let mut output = io::stderr();
        write!(output, "    {step}")?;
        writeln!(output, " {msg}")?;
        output.flush()?;
        Ok(())
    }
}

/// Prints a warning message to stderr, in the same format that `cargo` uses.
pub fn print_warning(message: &str) -> crate::Result<()> {
    if let Some(mut output) = term::stderr() {
        safe_term_attr(&mut output, term::Attr::Bold)?;
        if output.supports_color() {
            output.fg(term::color::YELLOW)?;
        }
        write!(output, "warning:")?;
        output.reset()?;
        writeln!(output, " {message}")?;
        output.flush()?;
        Ok(())
    } else {
        let mut output = io::stderr();
        write!(output, "warning:")?;
        writeln!(output, " {message}")?;
        output.flush()?;
        Ok(())
    }
}

/// Prints an error to stderr, in the same format that `cargo` uses.
pub fn print_error(error: &anyhow::Error) -> crate::Result<()> {
    if let Some(mut output) = term::stderr() {
        safe_term_attr(&mut output, term::Attr::Bold)?;
        if output.supports_color() {
            output.fg(term::color::RED)?;
        }
        write!(output, "error:")?;
        if output.supports_reset() {
            output.reset()?;
        }
        safe_term_attr(&mut output, term::Attr::Bold)?;
        writeln!(output, " {error}")?;
        if output.supports_reset() {
            output.reset()?;
        }
        for cause in error.chain().skip(1) {
            writeln!(output, "  Caused by: {cause}")?;
        }
        let backtrace = error.backtrace();
        writeln!(output, "{backtrace:?}")?;
        output.flush()?;
        Ok(())
    } else {
        let mut output = io::stderr();
        write!(output, "error:")?;
        writeln!(output, " {error}")?;
        for cause in error.chain().skip(1) {
            writeln!(output, "  Caused by: {cause}")?;
        }
        let backtrace = error.backtrace();
        writeln!(output, "{backtrace:?}")?;
        output.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_dir, create_file, is_retina, read_file, resource_relpath, symlink_file};

    use std::io::Write;
    use std::path::{Path, PathBuf};

    #[test]
    fn create_file_with_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!tmp.path().join("parent").exists());
        {
            let mut file = create_file(&tmp.path().join("parent/file.txt")).unwrap();
            writeln!(file, "Hello, world!").unwrap();
        }
        assert!(tmp.path().join("parent").is_dir());
        assert!(tmp.path().join("parent/file.txt").is_file());
    }

    #[test]
    fn copy_dir_with_symlinks() {
        // Create a directory structure that looks like this:
        //   ${TMP}/orig/
        //       sub/
        //           file.txt
        //       link -> sub/file.txt
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut file = create_file(&tmp.path().join("orig/sub/file.txt")).unwrap();
            writeln!(file, "Hello, world!").unwrap();
        }
        symlink_file(
            &tmp.path().join("orig/sub/file.txt"),
            &tmp.path().join("orig/link"),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(tmp.path().join("orig/link"))
                .unwrap()
                .as_slice(),
            b"Hello, world!\n"
        );
        // Copy ${TMP}/orig to ${TMP}/parent/copy, and make sure that the
        // directory structure, file, and symlink got copied correctly.
        copy_dir(&tmp.path().join("orig"), &tmp.path().join("parent/copy")).unwrap();
        assert!(tmp.path().join("parent/copy").is_dir());
        assert!(tmp.path().join("parent/copy/sub").is_dir());
        assert!(tmp.path().join("parent/copy/sub/file.txt").is_file());
        assert_eq!(
            std::fs::read(tmp.path().join("parent/copy/sub/file.txt"))
                .unwrap()
                .as_slice(),
            b"Hello, world!\n"
        );
        assert!(tmp.path().join("parent/copy/link").exists());
        assert_eq!(
            std::fs::read_link(tmp.path().join("parent/copy/link")).unwrap(),
            tmp.path().join("orig/sub/file.txt")
        );
        assert_eq!(
            std::fs::read(tmp.path().join("parent/copy/link"))
                .unwrap()
                .as_slice(),
            b"Hello, world!\n"
        );
    }

    #[test]
    fn retina_icon_paths() {
        assert!(!is_retina("data/icons/512x512.png"));
        assert!(is_retina("data/icons/512x512@2x.png"));
    }

    #[test]
    fn resource_relative_paths() {
        assert_eq!(
            resource_relpath(&PathBuf::from("./data/images/button.png")),
            PathBuf::from("data/images/button.png")
        );
        assert_eq!(
            resource_relpath(&PathBuf::from("../../images/wheel.png")),
            PathBuf::from("_up_/_up_/images/wheel.png")
        );
        assert_eq!(
            resource_relpath(&PathBuf::from("/home/ferris/crab.png")),
            PathBuf::from("_root_/home/ferris/crab.png")
        );
    }

    #[test]
    fn read_files() {
        const HELLO_WORLD: &str = "Hello, world!";
        const FILE: &str = "some/sub/file.txt";
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut file = create_file(&tmp.path().join(FILE)).unwrap();
            write!(file, "{HELLO_WORLD}").unwrap();
        }

        // Happy path
        let read = read_file(&tmp.path().join(FILE)).unwrap();
        assert_eq!(read, HELLO_WORLD);

        // Fail to find file
        assert!(read_file(&tmp.path().join("other/path")).is_err());

        // Find dir instead of file
        assert!(read_file(&tmp.path().join(Path::new(FILE).parent().unwrap())).is_err());
    }
}
