use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const BUILD_PROFILE_ENV: &str = "THREADPLANE_BUILD_PROFILE";
const GIT_COMMIT_ENV: &str = "THREADPLANE_GIT_COMMIT";
const GIT_DIRTY_ENV: &str = "THREADPLANE_GIT_DIRTY";

#[inline]
pub fn emit_build_metadata() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PROFILE");

    if let Ok(profile) = env::var("PROFILE") {
        println!("cargo:rustc-env={BUILD_PROFILE_ENV}={profile}");
    }

    emit_git_rerun_hints();

    if let Some(commit) = git_output(["rev-parse", "--short=12", "HEAD"]) {
        println!("cargo:rustc-env={GIT_COMMIT_ENV}={commit}");
    }

    println!("cargo:rustc-env={GIT_DIRTY_ENV}={}", git_is_dirty());
}

fn emit_git_rerun_hints() {
    let Some(git_dir) = git_output(["rev-parse", "--git-dir"]).map(PathBuf::from) else {
        return;
    };

    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());

    let Ok(head_contents) = fs::read_to_string(&head_path) else {
        return;
    };
    let Some(reference) = head_contents.strip_prefix("ref: ").map(str::trim) else {
        return;
    };

    let ref_path = normalize_git_path(&git_dir, Path::new(reference));
    println!("cargo:rerun-if-changed={}", ref_path.display());
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn git_is_dirty() -> bool {
    let Some(status) = Command::new("git")
        .args(["status", "--short", "--untracked-files=no"])
        .output()
        .ok()
    else {
        return false;
    };

    status.status.success() && !status.stdout.is_empty()
}

fn normalize_git_path(base: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base.join(candidate)
    }
}
