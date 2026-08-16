//! Shared install-contribution helpers.

use std::path::PathBuf;

pub(crate) fn push_named_bins(paths: &mut Vec<PathBuf>, dir: PathBuf, name: &str) {
    #[cfg(windows)]
    {
        paths.push(dir.join(format!("{name}.exe")));
        paths.push(dir.join(format!("{name}.cmd")));
    }
    paths.push(dir.join(name));
}
