//! LAAB, a LINE client for the desktop. Tauri shell around line-rs.

mod actions;
mod analytics;
mod cache;
mod commands;
mod convert;
mod crash;
mod demo;
mod fold;
mod keychain;
mod liff;
mod liff_devtools;
mod line;
mod logfile;
mod media;
mod model;
mod notify;
mod paths;
mod reads;
mod report;
mod richmenu;
mod session;
mod state;
mod stickers;
mod store;
mod traffic_lights;
mod usage;
mod vault;

use std::sync::Arc;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use crate::state::AppState;

/// `laab://chat/<chatId>?message=<messageId>` (used by the Raycast extension).
fn open_deep_link(app: &tauri::AppHandle, url: &str) {
    let Ok(u) = url::Url::parse(url) else { return };
    if u.scheme() != "laab" || u.host_str() != Some("chat") {
        return;
    }
    let Some(chat) = u
        .path_segments()
        .and_then(|mut s| s.next())
        .filter(|s| !s.is_empty())
        .map(media::decode)
    else {
        return;
    };
    let message = u
        .query_pairs()
        .find(|(k, _)| k == "message")
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty());
    usage::hit(usage::Action::OpenedDeepLink);
    show_in_main(app, &chat, message.as_deref());
}

/// Bring the main window forward on a chat, scrolled to `message` if given.
pub(crate) fn show_in_main(app: &tauri::AppHandle, chat: &str, message: Option<&str>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(
        "open-chat",
        serde_json::json!({ "chatId": chat, "messageId": message }),
    );
}

/// Set when the database couldn't be opened and was moved aside, to report
/// once the user's choice about statistics can be read.
static SET_ASIDE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Open the encrypted database (demo data stays unencrypted) and move any
/// plain session files into it.
fn open_store(paths: &paths::Paths, demo: bool) -> anyhow::Result<store::Store> {
    let db = paths.database();
    if demo {
        vault::init(None);
        return store::Store::open(&db, None);
    }
    let key = keychain::database_key()?;
    vault::init(Some(&key));
    let store = match store::Store::open(&db, Some(&key)) {
        Err(e) if store::is_wrong_key(&e) => {
            // The key that wrote this database is gone. Keep the file in case
            // the key comes back and start empty; chats sync again from LINE.
            let aside = db.with_extension(format!("sqlite.unreadable-{}", state::now_ms() as u64));
            tracing::warn!("database key does not match; moved it to {}", aside.display());
            SET_ASIDE.store(true, std::sync::atomic::Ordering::Relaxed);
            std::fs::rename(&db, &aside)?;
            for suffix in ["-wal", "-shm"] {
                let mut side = db.as_os_str().to_owned();
                side.push(suffix);
                let _ = std::fs::remove_file(side);
            }
            store::Store::open(&db, Some(&key))?
        }
        result => result?,
    };
    session::import_files(&store, paths)?;
    Ok(store)
}

/// A modal alert for errors that stop the app before its window opens.
fn alert(message: &str, info: &str) {
    tracing::error!("{message}: {info}");
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSAlert;
        use objc2_foundation::{MainThreadMarker, NSString};
        if let Some(mtm) = MainThreadMarker::new() {
            let alert = NSAlert::new(mtm);
            alert.setMessageText(&NSString::from_str(message));
            alert.setInformativeText(&NSString::from_str(info));
            alert.runModal();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tracing_subscriber::prelude::*;
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "laab_lib=info,line_rs=warn".into())
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(logfile::Writer)
                .with_filter(filter()),
        )
        .init();
    tracing::info!("LAAB starting");
    let demo = std::env::args().any(|a| a == "--demo") || std::env::var_os("LAAB_DEMO").is_some();

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_denylist(&[])
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol("media", |ctx, request, responder| {
            let state = ctx.app_handle().state::<Arc<AppState>>().inner().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(media::handle(state, request).await);
            });
        })
        .setup(move |app| {
            let paths = paths::Paths::resolve(demo)?;
            logfile::start(&paths.root);
            crash::install(&paths.root);
            let mut store = match open_store(&paths, demo) {
                Ok(store) => store,
                Err(e) => {
                    alert("เปิดข้อมูล LAAB ไม่ได้", &format!("{e:#}"));
                    return Err(e.into());
                }
            };
            if demo {
                demo::seed(&mut store, &paths)?;
            }
            let state = AppState::new(app.handle().clone(), paths, demo, store);
            app.manage(state.clone());
            analytics::startup(&state);
            if SET_ASIDE.load(std::sync::atomic::Ordering::Relaxed) {
                analytics::failed(&state, analytics::Failure::DatabaseUnreadable, "");
            }
            notify::setup(&app.config().identifier);
            if let Some(w) = app.get_webview_window("main") {
                traffic_lights::place(&w.as_ref().window());
            }

            if demo {
                state.update_session(|s| {
                    s.state = "demo".into();
                    s.status = "โหมด demo".into();
                    s.profile = Some(model::Profile {
                        mid: "me".into(),
                        display_name: "Nara".into(),
                        picture: None,
                        device: "DEMO".into(),
                    });
                });
            } else {
                cache::startup(state.clone());
                let s = state.clone();
                tauri::async_runtime::spawn(async move { line::start(s).await });
                notify::update_badge(&state);
            }

            use tauri_plugin_deep_link::DeepLinkExt;
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    open_deep_link(&handle, url.as_str());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::Resized(_) = event {
                traffic_lights::place(window);
            }
            if let WindowEvent::Focused(active) = event {
                let state = window.app_handle().state::<Arc<AppState>>();
                state.focus.lock().unwrap().active = *active;
                let _ = window.emit("app-active", *active);
            }
        })
        .invoke_handler(tauri::generate_handler![
            fold::resize_window,
            fold::release_web_view,
            commands::session,
            commands::login_qr,
            commands::logout,
            commands::chats,
            commands::people,
            commands::messages,
            commands::reads,
            commands::members,
            commands::set_focus,
            commands::open_chat,
            commands::mark_read,
            commands::refresh,
            commands::reload_chat,
            commands::send_text,
            commands::send_sticker,
            commands::stickers,
            commands::send_files,
            commands::download,
            commands::react,
            commands::set_muted,
            commands::set_pinned,
            commands::add_workspace,
            commands::create_folder,
            commands::move_to_folder,
            commands::rename_folder,
            commands::delete_folder,
            commands::search,
            commands::draft,
            commands::set_draft,
            commands::raw_message,
            commands::send_postback,
            commands::open_liff,
            commands::liff_dev_sessions,
            commands::liff_dev_action,
            commands::liff_dev_eval,
            commands::open_liff_devtools,
            commands::liff_share,
            commands::open_chat_window,
            commands::is_demo,
            commands::analytics_status,
            commands::set_analytics,
            commands::report_preview,
            commands::send_report,
            commands::report_js_error,
            commands::open_path,
            commands::reveal_path,
            commands::cache_size,
            commands::clear_cache,
            commands::announcements,
            commands::rich_menu,
            commands::login_password,
            commands::unsend,
            commands::edit_message,
            commands::translate,
            commands::save_to_keep,
            commands::hide_chat,
            commands::delete_history,
            commands::remove_announcement,
            commands::announce,
            commands::set_blocked,
            commands::find_contact,
            commands::contact_profile,
            commands::add_friend,
            commands::join_group_link,
            commands::list_notes,
            commands::create_note,
            commands::delete_note,
            commands::list_albums,
            commands::album_photos,
            commands::create_album,
            commands::rename_album,
            commands::delete_album,
            commands::delete_message,
            commands::leave_group,
            commands::chat_media,
            commands::save_text,
            commands::save_attachment,
            commands::open_aux_window,
            commands::open_viewer,
            commands::viewer_payload,
            commands::show_message,
            commands::older_messages,
            commands::quit,
            commands::note_detail,
            commands::add_comment,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LAAB")
        .run(|app, event| {
            // Clicking the Dock icon with no window open brings the main one back.
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
                && let Some(w) = app.get_webview_window("main")
            {
                let _ = w.show();
                let _ = w.set_focus();
            }
            if let RunEvent::Exit = event {
                cache::clear_open_copies();
            }
            let _ = (app, event);
        });
}
