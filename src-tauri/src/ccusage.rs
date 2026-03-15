use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;

/// Manages ccusage installation and execution with caching.
pub struct CcusageRunner {
    install_dir: PathBuf,
    cache_dir: PathBuf,
    node_path: Option<PathBuf>,
    /// In-memory cache: cache_key → (json_data, cached_at)
    /// Uses Mutex for interior mutability so run_cached can work with &self.
    mem_cache: Mutex<HashMap<String, (String, Instant)>>,
    /// Per-key spawn locks: prevents duplicate concurrent subprocesses
    /// for the same cache key when multiple callers race past expired cache.
    spawn_locks: Mutex<HashMap<String, Arc<TokioMutex<()>>>>,
}

impl CcusageRunner {
    pub fn new() -> Self {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("com.tokenmonitor.app");

        Self {
            cache_dir: base.join("cache"),
            install_dir: base.clone(),
            node_path: None,
            mem_cache: Mutex::new(HashMap::new()),
            spawn_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Look up a binary by checking common Homebrew/system paths, then `which`.
    fn find_binary(name: &str) -> Option<PathBuf> {
        for dir in ["/usr/local/bin", "/opt/homebrew/bin"] {
            let p = PathBuf::from(dir).join(name);
            if p.exists() {
                return Some(p);
            }
        }
        std::process::Command::new("which")
            .arg(name)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !s.is_empty() {
                        return Some(PathBuf::from(s));
                    }
                }
                None
            })
    }

    /// Ensure ccusage + @ccusage/codex are installed locally
    pub async fn ensure_installed(&mut self) -> Result<(), String> {
        self.node_path = Self::find_binary("node");
        if self.node_path.is_none() {
            return Err("Node.js not found. Please install Node.js to use TokenMonitor.".into());
        }

        // Create dirs
        std::fs::create_dir_all(&self.install_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| e.to_string())?;

        // Check if already installed
        let ccusage_bin = self.ccusage_bin_path();
        if ccusage_bin.exists() {
            return Ok(());
        }

        // Install ccusage and @ccusage/codex
        let npm_path = Self::find_binary("npm").ok_or("npm not found")?;
        let output = Command::new(&npm_path)
            .args([
                "install",
                "--prefix",
                self.install_dir.to_str().unwrap(),
                "ccusage",
                "@ccusage/codex",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run npm install: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("npm install failed: {}", stderr));
        }

        Ok(())
    }

    fn ccusage_bin_path(&self) -> PathBuf {
        self.install_dir.join("node_modules/.bin/ccusage")
    }

    fn codex_bin_path(&self) -> PathBuf {
        self.install_dir.join("node_modules/.bin/ccusage-codex")
    }

    /// Run ccusage with caching. Returns (JSON string, from_cache).
    ///
    /// Uses a 3-tier cache hierarchy for minimal latency:
    ///   1. In-memory HashMap (nanoseconds)
    ///   2. Disk JSON file (milliseconds)
    ///   3. CLI subprocess (seconds)
    ///
    /// On CLI failure, falls back to stale cached data for stability.
    pub async fn run_cached(
        &self,
        provider: &str,
        subcommand: &str,
        extra_args: &[&str],
        max_age: Duration,
    ) -> Result<(String, bool), String> {
        let args_key: String = extra_args.join("-");
        let cache_key = format!("{}-{}-{}", provider, subcommand, args_key);
        let cache_file = self.cache_dir.join(format!("{}.json", cache_key));

        // Tier 1: In-memory cache (fastest path)
        {
            let cache = self.mem_cache.lock().unwrap();
            if let Some((data, cached_at)) = cache.get(&cache_key) {
                if cached_at.elapsed() < max_age {
                    return Ok((data.clone(), true));
                }
            }
        }

        // Tier 2: Disk cache
        if let Ok(meta) = std::fs::metadata(&cache_file) {
            if let Ok(modified) = meta.modified() {
                if SystemTime::now().duration_since(modified).unwrap_or(Duration::MAX) < max_age {
                    if let Ok(cached) = std::fs::read_to_string(&cache_file) {
                        // Promote to in-memory cache for future reads
                        {
                            let mut cache = self.mem_cache.lock().unwrap();
                            cache.insert(cache_key.clone(), (cached.clone(), Instant::now()));
                        }
                        return Ok((cached, true));
                    }
                }
            }
        }

        // Tier 3: CLI subprocess — acquire per-key lock to prevent
        // duplicate spawns when multiple callers race past expired cache.
        let key_lock = {
            let mut locks = self.spawn_locks.lock().unwrap();
            locks
                .entry(cache_key.clone())
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };
        let _guard = key_lock.lock().await;

        // Double-check: the winner may have already populated the cache
        {
            let cache = self.mem_cache.lock().unwrap();
            if let Some((data, cached_at)) = cache.get(&cache_key) {
                if cached_at.elapsed() < max_age {
                    return Ok((data.clone(), true));
                }
            }
        }

        let json = match self.run_fresh(provider, subcommand, extra_args).await {
            Ok(j) => j,
            Err(e) => {
                // Stability: fall back to stale cache instead of surfacing error
                if let Ok(stale) = std::fs::read_to_string(&cache_file) {
                    return Ok((stale, true));
                }
                return Err(e);
            }
        };

        // Update both cache tiers
        {
            let mut cache = self.mem_cache.lock().unwrap();
            cache.insert(cache_key, (json.clone(), Instant::now()));
        }
        let _ = std::fs::write(&cache_file, &json);

        Ok((json, false))
    }

    /// Execute ccusage/codex subprocess and return stdout
    async fn run_fresh(
        &self,
        provider: &str,
        subcommand: &str,
        extra_args: &[&str],
    ) -> Result<String, String> {
        let node = self
            .node_path
            .as_ref()
            .ok_or("Node.js not initialized")?;

        let bin = match provider {
            "codex" => self.codex_bin_path(),
            _ => self.ccusage_bin_path(),
        };

        if !bin.exists() {
            return Err(format!("ccusage binary not found at {:?}", bin));
        }

        let mut cmd = Command::new(node);
        cmd.arg(&bin);
        cmd.arg(subcommand);
        cmd.args(["--json", "--offline"]);
        cmd.args(extra_args);

        // Inherit PATH for node resolution
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to execute ccusage: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ccusage exited with error: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Invalidate all cache entries (both in-memory and disk)
    pub fn clear_cache(&self) {
        {
            let mut cache = self.mem_cache.lock().unwrap();
            cache.clear();
        }
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Update ccusage packages in background
    pub async fn update_packages(&self) -> Result<(), String> {
        let npm = Self::find_binary("npm").ok_or("npm not found")?;
        let _ = Command::new(&npm)
            .args([
                "update",
                "--prefix",
                self.install_dir.to_str().unwrap(),
            ])
            .output()
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_runner(dir: &TempDir) -> CcusageRunner {
        let base = dir.path().to_path_buf();
        fs::create_dir_all(base.join("cache")).unwrap();
        CcusageRunner {
            install_dir: base.clone(),
            cache_dir: base.join("cache"),
            node_path: None,
            mem_cache: Mutex::new(HashMap::new()),
            spawn_locks: Mutex::new(HashMap::new()),
        }
    }

    // ── Cache key generation ──

    #[test]
    fn cache_key_format() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);
        // Verify the cache file path derivation matches the key pattern
        let expected_file = runner.cache_dir.join("claude-daily---since-20260315.json");
        // Replicate the key logic from run_cached
        let args_key: String = ["--since", "20260315"].join("-");
        let cache_key = format!("{}-{}-{}", "claude", "daily", args_key);
        let cache_file = runner.cache_dir.join(format!("{}.json", cache_key));
        assert_eq!(cache_file, expected_file);
    }

    // ── In-memory cache ──

    #[tokio::test]
    async fn mem_cache_hit_within_ttl() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);

        // Manually insert into mem cache
        {
            let mut cache = runner.mem_cache.lock().unwrap();
            cache.insert(
                "claude-daily---since-20260315".to_string(),
                (r#"{"daily":[]}"#.to_string(), Instant::now()),
            );
        }

        let result = runner
            .run_cached("claude", "daily", &["--since", "20260315"], Duration::from_secs(60))
            .await;
        let (data, from_cache) = result.unwrap();
        assert!(from_cache);
        assert_eq!(data, r#"{"daily":[]}"#);
    }

    #[tokio::test]
    async fn mem_cache_miss_after_expiry() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);

        // Insert with an already-expired instant
        {
            let mut cache = runner.mem_cache.lock().unwrap();
            cache.insert(
                "test-sub-".to_string(),
                ("old".to_string(), Instant::now() - Duration::from_secs(300)),
            );
        }

        // No disk cache, no node binary -> should error (falls through all tiers)
        let result = runner
            .run_cached("test", "sub", &[], Duration::from_secs(60))
            .await;
        assert!(result.is_err());
    }

    // ── Disk cache ──

    #[tokio::test]
    async fn disk_cache_hit_promotes_to_memory() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);

        // Write a cache file
        let cache_file = runner.cache_dir.join("claude-blocks---since-20260315.json");
        fs::write(&cache_file, r#"{"blocks":[]}"#).unwrap();

        let result = runner
            .run_cached("claude", "blocks", &["--since", "20260315"], Duration::from_secs(60))
            .await;
        let (data, from_cache) = result.unwrap();
        assert!(from_cache);
        assert_eq!(data, r#"{"blocks":[]}"#);

        // Verify it was promoted to in-memory cache
        let cache = runner.mem_cache.lock().unwrap();
        assert!(cache.contains_key("claude-blocks---since-20260315"));
    }

    // ── clear_cache ──

    #[test]
    fn clear_cache_empties_both_tiers() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);

        // Populate memory
        {
            let mut cache = runner.mem_cache.lock().unwrap();
            cache.insert("key1".into(), ("data".into(), Instant::now()));
            cache.insert("key2".into(), ("data".into(), Instant::now()));
        }

        // Populate disk
        fs::write(runner.cache_dir.join("a.json"), "{}").unwrap();
        fs::write(runner.cache_dir.join("b.json"), "{}").unwrap();

        runner.clear_cache();

        let cache = runner.mem_cache.lock().unwrap();
        assert!(cache.is_empty());

        let files: Vec<_> = fs::read_dir(&runner.cache_dir)
            .unwrap()
            .collect();
        assert!(files.is_empty());
    }

    // ── Spawn deduplication ──

    /// Regression test: concurrent callers that all miss cache must only
    /// spawn ONE subprocess per cache key, not N.  Before the per-key
    /// spawn lock was added, this would spawn 5 duplicate processes.
    #[tokio::test]
    async fn concurrent_expired_cache_deduplicates_spawns() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().to_path_buf();
        fs::create_dir_all(base.join("cache")).unwrap();
        fs::create_dir_all(base.join("node_modules/.bin")).unwrap();

        let node = match CcusageRunner::find_binary("node") {
            Some(n) => n,
            None => return, // skip if node not installed
        };

        // Fake ccusage: atomically increments a counter file, then
        // delays 200ms before printing JSON so the race window is wide.
        fs::write(
            base.join("node_modules/.bin/ccusage"),
            r#"
const fs = require('fs'), p = require('path');
const f = p.join(p.resolve(__dirname, '..', '..'), '.spawn_count');
let n = 0;
try { n = parseInt(fs.readFileSync(f, 'utf8')); } catch {}
fs.writeFileSync(f, String(n + 1));
setTimeout(() => console.log('{"daily":[]}'), 200);
"#,
        )
        .unwrap();

        let runner = CcusageRunner {
            install_dir: base.clone(),
            cache_dir: base.join("cache"),
            node_path: Some(node),
            mem_cache: Mutex::new(HashMap::new()),
            spawn_locks: Mutex::new(HashMap::new()),
        };
        let ttl = Duration::from_secs(60);

        // 5 concurrent calls for the same cache key
        let (r1, r2, r3, r4, r5) = tokio::join!(
            runner.run_cached("claude", "daily", &["--since", "20260315"], ttl),
            runner.run_cached("claude", "daily", &["--since", "20260315"], ttl),
            runner.run_cached("claude", "daily", &["--since", "20260315"], ttl),
            runner.run_cached("claude", "daily", &["--since", "20260315"], ttl),
            runner.run_cached("claude", "daily", &["--since", "20260315"], ttl),
        );

        // All must succeed
        for (i, r) in [&r1, &r2, &r3, &r4, &r5].iter().enumerate() {
            assert!(r.is_ok(), "call {i} failed: {:?}", r.as_ref().err());
        }

        // Exactly 1 subprocess, not 5
        let count: i32 = fs::read_to_string(base.join(".spawn_count"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(count, 1, "expected 1 spawn, got {count}");
    }

    // ── Binary paths ──

    #[test]
    fn ccusage_bin_path_is_under_node_modules() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);
        assert!(runner.ccusage_bin_path().ends_with("node_modules/.bin/ccusage"));
    }

    #[test]
    fn codex_bin_path_is_under_node_modules() {
        let dir = TempDir::new().unwrap();
        let runner = make_runner(&dir);
        assert!(runner.codex_bin_path().ends_with("node_modules/.bin/ccusage-codex"));
    }
}
