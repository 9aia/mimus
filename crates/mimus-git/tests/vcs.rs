//! GitVcs against real temp repos: commits, tags, worktrees.

use mimus_git::GitVcs;
use mimus_vcs::Vcs;
use std::path::PathBuf;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let o = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {}", args.join(" "));
}

fn repo(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mimus-git-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init", "--allow-empty"]);
    d
}

#[test]
fn has_commit_and_tags() {
    let repo = repo("t");
    let v = GitVcs;
    let o = Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "HEAD"]).output().unwrap();
    let sha = String::from_utf8_lossy(&o.stdout).trim().to_string();
    assert!(v.has_commit(&repo, &sha));
    assert!(!v.has_commit(&repo, "0000000000000000000000000000000000000000"));

    assert!(!v.tag_exists(&repo, "v1"));
    assert!(v.tag(&repo, "v1"));
    assert!(v.tag_exists(&repo, "v1"));
    // push_tag to nowhere fails cleanly (false, not panic).
    assert!(!v.push_tag(&repo, "v1"));
}

#[test]
fn worktree_lifecycle() {
    let repo = repo("wt");
    let v = GitVcs;
    let o = Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "HEAD"]).output().unwrap();
    let sha = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let wt = repo.join("wt-dest");
    assert!(v.worktree_add(&repo, &wt, &sha));
    assert!(wt.join(".git").exists());
    assert!(!v.worktree_add(&repo, &wt, &sha)); // already exists → false
    assert!(v.worktree_remove(&repo, &wt));
    assert!(!wt.exists());
    assert!(v.worktree_prune(&repo));
    // sync without a remote fails clean.
    assert!(!v.sync(&repo));
}
