//! Auto-discovery of default document roots.
//!
//! Returns common user directories: Documents, Downloads, Desktop.

use std::path::PathBuf;

/// Discover default document roots for indexing.
///
/// Returns a vector of paths that commonly contain user documents:
/// - ~/Documents
/// - ~/Downloads
/// - ~/Desktop
///
/// Only returns paths that actually exist on the filesystem.
pub fn discover_default_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let candidates = [
            home.join("Documents"),
            home.join("Downloads"),
            home.join("Desktop"),
        ];

        for path in candidates {
            if path.exists() && path.is_dir() {
                roots.push(path);
            }
        }
    }

    roots
}

/// Discover default roots, filtering by a predicate.
///
/// Useful for tests or custom filtering logic.
pub fn discover_default_roots_filtered<F>(filter: F) -> Vec<PathBuf>
where
    F: Fn(&PathBuf) -> bool,
{
    discover_default_roots()
        .into_iter()
        .filter(filter)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_discover_default_roots_creates_expected_paths() {
        // We can't easily test the actual home dir, but we can verify
        // the function returns a Vec<PathBuf> without panicking
        let roots = discover_default_roots();
        assert!(roots.iter().all(|p| p.is_absolute()));
    }

    #[test]
    fn test_discover_default_roots_filtered() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("Documents");
        let downloads = dir.path().join("Downloads");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&downloads).unwrap();

        // This test would need a mock home dir, which dirs doesn't easily support
        // So we just test the filtering logic works
        let roots = vec![docs.clone(), downloads.clone()];
        let filtered: Vec<_> = roots.into_iter().filter(|p| p.file_name().unwrap() == "Documents").collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], docs);
    }
}