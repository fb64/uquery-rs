use self_update::backends::github::Update;
use self_update::update::{Release, UpdateStrategy};
use self_update::{cargo_crate_version, get_target};
use std::path::Path;

const REPO_OWNER: &str = "fb64";
const REPO_NAME: &str = "uquery-rs";

fn updater(no_confirm: bool) -> Result<Update, String> {
    Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("uquery")
        .current_version(cargo_crate_version!())
        // 0.x minor bumps are semver-incompatible, the default strategy would skip them.
        .update_strategy(UpdateStrategy::Latest)
        .show_download_progress(true)
        .no_confirm(no_confirm)
        .build()
        .map_err(|e| e.to_string())
}

fn release_url(release: &Release) -> String {
    release
        .release_notes_url()
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/tag/v{}",
                release.version()
            )
        })
}

fn print_release(release: &Release) {
    println!(
        "New uQuery version available: v{} (current: v{})",
        release.version(),
        cargo_crate_version!()
    );
    println!("Release page: {}", release_url(release));
    if let Some(notes) = release.body().filter(|b| !b.trim().is_empty()) {
        println!("\nRelease notes:\n{}\n", notes.trim());
    }
}

/// Checks GitHub for a newer release and prints its release notes. Unless
/// `check_only` is set, replaces the running binary with the release asset
/// matching the current target.
pub fn run(check_only: bool, no_confirm: bool) -> Result<(), String> {
    if Path::new("/.dockerenv").exists() {
        return Err("self-update is not supported in Docker, pull the latest image instead: docker pull fb64/uquery".to_string());
    }
    let updater = updater(no_confirm)?;
    let releases = updater.get_newer_releases().map_err(|e| e.to_string())?;
    let Some(release) = releases.latest() else {
        println!("uQuery v{} is up to date.", cargo_crate_version!());
        return Ok(());
    };
    print_release(release);
    if check_only {
        println!("Run `uquery --self-update` to install it.");
        return Ok(());
    }
    if !release.has_target_asset(get_target()) {
        return Err(format!(
            "release v{} has no binary for target {}",
            release.version(),
            get_target()
        ));
    }
    let status = updater.update().map_err(|e| e.to_string())?;
    if status.is_updated() {
        println!("uQuery updated to v{}.", status.version());
        println!(
            "If you pre-installed DuckDB extensions, run `uquery --install-extensions` again: \
             extensions are tied to the bundled DuckDB version."
        );
    }
    Ok(())
}
