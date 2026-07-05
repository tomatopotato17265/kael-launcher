use crate::State;
use crate::event::ProfilePayloadType;
use crate::event::emit::{emit_profile, emit_warning};
use crate::state::{
    DirectoryInfo, ProfileInstallStage, ProjectType, attached_world_data,
};
use crate::worlds::WorldType;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::time::Duration;
use tokio::sync::{RwLock, mpsc::channel};

pub type FileWatcher = RwLock<Debouncer<RecommendedWatcher>>;

pub async fn init_watcher() -> crate::Result<FileWatcher> {
    let (tx, mut rx) = channel(1);

    let file_watcher = new_debouncer(
        Duration::from_secs_f32(1.0),
        move |res: DebounceEventResult| {
            tx.blocking_send(res).ok();
        },
    )?;

    tokio::task::spawn(async move {
        let span = tracing::span!(tracing::Level::INFO, "init_watcher");
        tracing::info!(parent: &span, "Initing watcher");
        while let Some(res) = rx.recv().await {
            let _span = span.enter();

            match res {
                Ok(events) => {
                    if let Some(themes_dir) = resolve_themes_dir().await {
                        let mut changed_theme_file = None;
                        let mut theme_changed = false;
                        for e in events.iter() {
                            if e.path.starts_with(&themes_dir) {
                                theme_changed = true;
                                if changed_theme_file.is_none() {
                                    changed_theme_file = e
                                        .path
                                        .file_name()
                                        .map(|n| {
                                            n.to_string_lossy().to_string()
                                        });
                                }
                            }
                        }
                        if theme_changed {
                            tokio::spawn(async move {
                                let _ = crate::event::emit::emit_theme(
                                    changed_theme_file,
                                )
                                .await;
                            });
                        }
                    }

                    if let Some(fonts_dir) = resolve_fonts_dir().await {
                        let mut changed_font_file = None;
                        let mut font_changed = false;
                        for e in events.iter() {
                            if e.path.starts_with(&fonts_dir) {
                                font_changed = true;
                                if changed_font_file.is_none() {
                                    changed_font_file = e
                                        .path
                                        .file_name()
                                        .map(|n| {
                                            n.to_string_lossy().to_string()
                                        });
                                }
                            }
                        }
                        if font_changed {
                            tokio::spawn(async move {
                                let _ = crate::event::emit::emit_font(
                                    changed_font_file,
                                )
                                .await;
                            });
                        }
                    }

                    let mut visited_profiles = Vec::new();

                    events.iter().for_each(|e| {
                        let mut profile_path = None;

                        let mut found = false;
                        for component in e.path.components() {
                            if found {
                                profile_path = Some(component.as_os_str());
                                break;
                            }

                            if component.as_os_str()
                                == crate::state::dirs::PROFILES_FOLDER_NAME
                            {
                                found = true;
                            }
                        }

                        if let Some(profile_path) = profile_path {
                            let profile_path_str =
                                profile_path.to_string_lossy().to_string();
                            let first_file_name = e
                                .path
                                .components()
                                .skip_while(|x| x.as_os_str() != profile_path)
                                .nth(1)
                                .map(|x| x.as_os_str());
                            if first_file_name
                                .as_ref()
                                .is_some_and(|x| *x == "crash-reports")
                                && e.path
                                    .extension()
                                    .as_ref()
                                    .is_some_and(|x| *x == "txt")
                            {
                                crash_task(profile_path_str);
                            } else if !visited_profiles.contains(&profile_path)
                            {
                                let event = if first_file_name
                                    .as_ref()
                                    .is_some_and(|x| *x == "servers.dat")
                                {
                                    Some(ProfilePayloadType::ServersUpdated)
                                } else if first_file_name.as_ref().is_some_and(|x| {
                                    *x == "saves"
                                        && e.path
                                            .file_name()
                                            .as_ref()
                                            .is_some_and(|x| *x == "level.dat")
                                }) {
                                    tracing::info!(
                                        "World updated: {}",
                                        e.path.display()
                                    );
                                    let world = e
                                        .path
                                        .parent()
                                        .unwrap()
                                        .file_name()
                                        .unwrap()
                                        .to_string_lossy()
                                        .to_string();
                                    if !e.path.is_file() {
                                        let profile_path_str = profile_path_str.clone();
                                        let world = world.clone();
                                        tokio::spawn(async move {
                                            if let Ok(state) = State::get().await
                                                && let Err(e) = attached_world_data::AttachedWorldData::remove_for_world(
                                                    &profile_path_str,
                                                    WorldType::Singleplayer,
                                                    &world,
                                                    &state.pool
                                                ).await {
                                                    tracing::warn!("Failed to remove AttachedWorldData for '{world}': {e}")
                                                }
                                        });
                                    }
                                    Some(ProfilePayloadType::WorldUpdated { world })
                                } else if first_file_name
                                    .as_ref()
                                    .is_none_or(|x| *x != "saves")
                                {
                                    Some(ProfilePayloadType::Synced)
                                } else {
                                    None
                                };
                                if let Some(event) = event {
                                    tokio::spawn(async move {
                                        let _ = emit_profile(
                                            &profile_path_str,
                                            event,
                                        )
                                        .await;
                                    });
                                    visited_profiles.push(profile_path);
                                }
                            }
                        }
                    });
                }
                Err(error) => tracing::warn!("Unable to watch file: {error}"),
            }
        }
    });

    Ok(RwLock::new(file_watcher))
}

/// Resolves the configured themes directory (custom `theme_dir` setting, or
/// the default `config_dir/themes`) without creating it. Returns `None` if the
/// state or settings can't be read.
async fn resolve_themes_dir() -> Option<std::path::PathBuf> {
    let state = State::get().await.ok()?;
    let settings = crate::state::Settings::get(&state.pool).await.ok()?;
    Some(match settings.theme_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => state.directories.config_dir.join("themes"),
    })
}

/// Registers the themes directory with the watcher so live edits to theme
/// files are picked up. Safe to call repeatedly (e.g. after the themes folder
/// setting changes).
pub(crate) async fn watch_themes_dir(
    watcher: &FileWatcher,
    dir: &std::path::Path,
) {
    if let Err(e) = crate::util::io::create_dir_all(dir).await {
        tracing::error!("Failed to create themes dir for watcher {dir:?}: {e}");
        return;
    }

    let mut watcher = watcher.write().await;
    if let Err(e) = watcher.watcher().watch(dir, RecursiveMode::Recursive) {
        tracing::error!("Failed to watch themes directory {dir:?}: {e}");
    }
}

/// Resolves the configured fonts directory without creating it.
async fn resolve_fonts_dir() -> Option<std::path::PathBuf> {
    let state = State::get().await.ok()?;
    let settings = crate::state::Settings::get(&state.pool).await.ok()?;
    Some(match settings.font_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => state.directories.config_dir.join("fonts"),
    })
}

/// Registers the fonts directory with the watcher so dropped/edited font files
/// hot-reload. Safe to call repeatedly.
pub(crate) async fn watch_fonts_dir(
    watcher: &FileWatcher,
    dir: &std::path::Path,
) {
    if let Err(e) = crate::util::io::create_dir_all(dir).await {
        tracing::error!("Failed to create fonts dir for watcher {dir:?}: {e}");
        return;
    }

    let mut watcher = watcher.write().await;
    if let Err(e) = watcher.watcher().watch(dir, RecursiveMode::Recursive) {
        tracing::error!("Failed to watch fonts directory {dir:?}: {e}");
    }
}

/// Watches all existing profiles
pub(crate) async fn watch_profiles_init(
    watcher: &FileWatcher,
    dirs: &DirectoryInfo,
) {
    let Ok(mut profiles_dir) = tokio::fs::read_dir(dirs.profiles_dir()).await
    else {
        return;
    };

    while let Ok(Some(profile_dir)) = profiles_dir.next_entry().await {
        let file_name = profile_dir.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(".DS_Store") {
            continue;
        }

        watch_profile(&file_name, watcher, dirs).await;
    }
}

pub(crate) async fn watch_profile(
    profile_path: &str,
    watcher: &FileWatcher,
    dirs: &DirectoryInfo,
) {
    let profile_path = dirs.profiles_dir().join(profile_path);

    let Ok(metadata) = tokio::fs::metadata(&profile_path).await else {
        return;
    };

    if !metadata.is_dir() {
        return;
    }

    let mut to_watch = Vec::new();
    for sub_path in ProjectType::iterator()
        .map(|x| x.get_folder())
        .chain(["crash-reports", "saves"])
    {
        let full_path = profile_path.join(sub_path);

        let meta = tokio::fs::symlink_metadata(&full_path).await;
        let exists = meta.is_ok();
        let is_symlink = meta.ok().is_some_and(|m| m.file_type().is_symlink());

        if !exists
            && !is_symlink
            && !sub_path.contains(".")
            && let Err(e) = crate::util::io::create_dir_all(&full_path).await
        {
            tracing::error!(
                "Failed to create directory for watcher {full_path:?}: {e}"
            );
            return;
        }

        to_watch.push(full_path);
    }

    let mut watcher = watcher.write().await;
    for full_path in &to_watch {
        if let Err(e) =
            watcher.watcher().watch(full_path, RecursiveMode::Recursive)
        {
            tracing::error!(
                "Failed to watch directory for watcher {full_path:?}: {e}"
            );
            return;
        }
    }

    if let Err(e) = watcher
        .watcher()
        .watch(&profile_path, RecursiveMode::NonRecursive)
    {
        tracing::error!(
            "Failed to watch root profile directory for watcher {profile_path:?}: {e}"
        );
    }
}

fn crash_task(path: String) {
    tokio::task::spawn(async move {
        let res = async {
            let profile = crate::api::profile::get(&path).await?;

            if let Some(profile) = profile {
                // Hide warning if profile is not yet installed
                if profile.install_stage == ProfileInstallStage::Installed {
                    emit_warning(&format!("Profile {} has crashed! Visit the logs page to see a crash report.", profile.name)).await?;
                }
            }

            Ok::<(), crate::Error>(())
        }
            .await;

        match res {
            Ok(()) => {}
            Err(err) => {
                tracing::warn!("Unable to send crash report to frontend: {err}")
            }
        };
    });
}
