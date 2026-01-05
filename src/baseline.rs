use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::io::BufRead;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MaintainersEntry {
    pub subsystem: String,
    pub trees: Vec<String>,
    pub patterns: Vec<String>,
}

#[derive(Debug)]
pub struct BaselineRegistry {
    entries: Vec<MaintainersEntry>,
    remote_map: HashMap<String, String>, // URL -> Local Remote Name
}

impl BaselineRegistry {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let maintainers_path = repo_path.join("MAINTAINERS");
        let entries = if maintainers_path.exists() {
            info!("Loading MAINTAINERS from {:?}", maintainers_path);
            Self::parse_maintainers(&maintainers_path)?
        } else {
            warn!("MAINTAINERS file not found at {:?}, baseline detection will be limited", maintainers_path);
            Vec::new()
        };

        // Load local remotes
        // For now we just create an empty map, caller can populate it or we implement it here
        // Ideally we run `git remote -v` in repo_path.
        let remote_map = Self::load_git_remotes(repo_path).unwrap_or_default();

        Ok(Self { entries, remote_map })
    }

    fn parse_maintainers(path: &Path) -> Result<Vec<MaintainersEntry>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        
        let mut entries = Vec::new();
        let mut current_subsystem = String::new();
        let mut current_trees = Vec::new();
        let mut current_patterns = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                // End of entry
                if !current_subsystem.is_empty() && (!current_trees.is_empty() || !current_patterns.is_empty()) {
                    entries.push(MaintainersEntry {
                        subsystem: current_subsystem.clone(),
                        trees: current_trees.clone(),
                        patterns: current_patterns.clone(),
                    });
                }
                current_subsystem.clear();
                current_trees.clear();
                current_patterns.clear();
                continue;
            }

            if !line.contains(':') && current_subsystem.is_empty() {
                // Likely a subsystem header line (e.g. "NETWORKING [GENERAL]")
                current_subsystem = line.trim().to_string();
            } else if let Some((tag, value)) = line.split_once(':') {
                let val = value.trim();
                match tag {
                    "T" => {
                        if val.starts_with("git ") {
                            // Format: "git git://..." or "git https://..."
                            if let Some(url) = val.strip_prefix("git ") {
                                current_trees.push(url.trim().to_string());
                            }
                        }
                    },
                    "F" => {
                        current_patterns.push(val.to_string());
                    },
                    _ => {}
                }
            }
        }
        
        // Push last entry
        if !current_subsystem.is_empty() && (!current_trees.is_empty() || !current_patterns.is_empty()) {
            entries.push(MaintainersEntry {
                subsystem: current_subsystem,
                trees: current_trees,
                patterns: current_patterns,
            });
        }

        info!("Parsed {} MAINTAINERS entries", entries.len());
        Ok(entries)
    }

    fn load_git_remotes(repo_path: &Path) -> Result<HashMap<String, String>> {
        use std::process::Command;
        
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["remote", "-v"])
            .output()?;
            
        let mut map = HashMap::new();
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // origin  https://github.com/torvalds/linux.git (fetch)
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0];
                    let url = parts[1];
                    // Normalize URL? Remove .git suffix? 
                    // Maintainers usually has .git.
                    // Let's store exact URL and maybe normalized version.
                    map.insert(url.to_string(), name.to_string());
                    
                    // Also strip .git if present for robust matching
                    if let Some(stripped) = url.strip_suffix(".git") {
                        map.insert(stripped.to_string(), name.to_string());
                    }
                }
            }
        }
        Ok(map)
    }

    // Heuristics
    
    pub fn resolve_baseline(&self, files: &[String], subject: &str) -> String {
        // 1. Identify Candidate Trees
        let mut tree_counts: HashMap<String, usize> = HashMap::new();
        
        for file in files {
            for entry in &self.entries {
                // Check match
                let mut matched = false;
                for pattern in &entry.patterns {
                    // Simple prefix matching for directories, or exact match
                    // MAINTAINERS patterns are globs, but prefix is 90% case.
                    // e.g. "net/" matches "net/core/dev.c"
                    // "drivers/net/ethernet/intel/" matches ...
                    // "*" is rare but possible.
                    if pattern.ends_with('/') {
                        if file.starts_with(pattern) {
                            matched = true;
                            break;
                        }
                    } else if pattern == file {
                         matched = true;
                         break;
                    }
                    // TODO: Implement full glob if needed
                }
                
                if matched {
                    for tree in &entry.trees {
                        *tree_counts.entry(tree.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        if tree_counts.is_empty() {
            return "HEAD".to_string(); // Fallback
        }

        // 2. Filter/Sort Candidates
        // Heuristic: Prefer tree with most file matches
        let mut candidates: Vec<(&String, &usize)> = tree_counts.iter().collect();
        candidates.sort_by(|a, b| b.1.cmp(a.1)); // Descending count

        // 3. Subject Disambiguation
        // If we have top candidates, check subject for hints
        let subject_lower = subject.to_lowercase();
        
        // Keywords to prioritize
        let keywords = ["net", "bpf", "drm", "mm", "sched", "x86", "arm", "arm64", "scsi", "usb"];
        let is_next = subject_lower.contains("next");

        // Try to find a candidate that matches a keyword in subject
        for (url, _) in &candidates {
            // Check if URL contains a keyword present in subject
            for kw in keywords {
                if subject_lower.contains(kw) && url.contains(kw) {
                    // Strong match
                    if is_next && url.contains("next") {
                         return self.url_to_remote_ref(url);
                    }
                    if !is_next && !url.contains("next") {
                        // Prefer non-next if subject doesn't say next? 
                        // Actually, for dev, we usually want -next unless stated otherwise.
                        // But if subject has "bpf" and we have "bpf.git" and "bpf-next.git"...
                        // If subject says "bpf-next", we want bpf-next.
                        // If subject says "bpf", it's ambiguous.
                    }
                }
            }
            
            // Explicit check for -next preference
            if is_next && url.contains("next") {
                return self.url_to_remote_ref(url);
            }
        }

        // Default: Return the one with most matches
        // But prefer -next if counts are tied or close?
        // Let's just pick the top one.
        self.url_to_remote_ref(candidates[0].0)
    }

    fn url_to_remote_ref(&self, url: &str) -> String {
        // Map URL to local remote
        if let Some(remote_name) = self.remote_map.get(url) {
            format!("{}/master", remote_name) // Assuming master branch
        } else {
            // No local remote found for this URL.
            // We cannot return the URL because `GitWorktree` expects a commit-ish available locally.
            // Fallback to linux-next or HEAD?
            // Ideally we log this.
            warn!("Detected tree {} but no local remote found", url);
            
            // Try to guess remote name from URL basename? e.g. .../net-next.git -> net-next
            // And hope it exists? No, `remote_map` covers `git remote -v`.
            // So we definitely don't have it.
            
            // Fallback to "next/master" if available (linux-next is catch-all)
            // Or "origin/master" (mainline)
            // Or "HEAD".
            
            if self.remote_map.values().any(|v| v == "next") {
                "next/master".to_string()
            } else {
                "HEAD".to_string()
            }
        }
    }
}

// Helper to extract file paths from diff
pub fn extract_files_from_diff(diff: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("diff --git a/") {
            // path is like "drivers/net/foo.c b/drivers/net/foo.c"
            // We want "drivers/net/foo.c"
            if let Some((a, _)) = path.split_once(' ') {
                files.push(a.to_string());
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_registry() -> BaselineRegistry {
        let mut entries = Vec::new();
        
        entries.push(MaintainersEntry {
            subsystem: "NETWORKING".to_string(),
            trees: vec![
                "git://git.kernel.org/pub/scm/linux/kernel/git/netdev/net.git".to_string(),
                "git://git.kernel.org/pub/scm/linux/kernel/git/netdev/net-next.git".to_string(),
            ],
            patterns: vec!["net/".to_string(), "drivers/net/".to_string()],
        });

        entries.push(MaintainersEntry {
            subsystem: "BPF".to_string(),
            trees: vec![
                "git://git.kernel.org/pub/scm/linux/kernel/git/bpf/bpf.git".to_string(),
                "git://git.kernel.org/pub/scm/linux/kernel/git/bpf/bpf-next.git".to_string(),
            ],
            patterns: vec!["kernel/bpf/".to_string()],
        });

        let mut remote_map = HashMap::new();
        remote_map.insert("git://git.kernel.org/pub/scm/linux/kernel/git/netdev/net.git".to_string(), "net".to_string());
        remote_map.insert("git://git.kernel.org/pub/scm/linux/kernel/git/netdev/net-next.git".to_string(), "net-next".to_string());
        // BPF remotes missing to test fallback

        BaselineRegistry { entries, remote_map }
    }

    #[test]
    fn test_resolve_simple_match() {
        let registry = create_registry();
        let files = vec!["drivers/net/ethernet/intel/ice/ice_main.c".to_string()];
        
        // Should match NETWORKING.
        // Both net and net-next are candidates.
        // Default might pick first or random if counts equal.
        // But usually we prefer -next?
        // My implementation picks the first one from sorted list.
        // If counts are equal (1 each), sort order depends on internal iteration order or is unstable?
        // Actually, I put both trees in `trees` vector.
        // `tree_counts` will have {net: 1, net-next: 1}.
        // If subject is neutral, it might pick either.
        // Let's rely on Subject "net-next" to force it.
        
        let baseline = registry.resolve_baseline(&files, "[PATCH net-next] Fix ice driver");
        assert_eq!(baseline, "net-next/master");
    }

    #[test]
    fn test_resolve_fallback_no_remote() {
        let registry = create_registry();
        let files = vec!["kernel/bpf/core.c".to_string()];
        
        // Matches BPF.
        // Remote map missing BPF remotes.
        // Should fallback.
        // My logic: if remote_map has "next" value, use "next/master", else "HEAD".
        // In my mock, I have "net-next" as key and value.
        // Wait, remote_map values are remote names.
        // If I have a remote named "next", I use it.
        // My mock doesn't have "next" remote. It has "net" and "net-next".
        // So it should fallback to "HEAD".
        
        let baseline = registry.resolve_baseline(&files, "Fix bpf");
        assert_eq!(baseline, "HEAD");
    }

    #[test]
    fn test_resolve_no_match() {
        let registry = create_registry();
        let files = vec!["mm/mmap.c".to_string()]; // Not in my mock entries
        
        let baseline = registry.resolve_baseline(&files, "Fix mm");
        assert_eq!(baseline, "HEAD");
    }
}