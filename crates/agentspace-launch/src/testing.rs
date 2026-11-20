//! Throwaway folders for this crate's unit tests, under the temp folder and removed on drop.

use std::path::{Path, PathBuf};

pub(crate) struct Scratch {
    path: PathBuf,
}

impl Scratch {
    pub(crate) fn new(tag: &str) -> Scratch {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "agentspace-launch-unit-{tag}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Scratch { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The names in the folder, sorted.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&self.path)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
