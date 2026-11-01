//! LAAB's tab bar as a native `UITabBar` over the webview on iOS, so it gets
//! the system look (Liquid Glass on iOS 26 and later). The frontend sends the
//! whole state with `plugin:native-tabbar|update` and listens for `select`.
//! On other platforms the plugin does nothing and the frontend draws its own.

use tauri::{
    Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_native_tabbar);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("native-tabbar")
        .setup(|_app, _api| {
            #[cfg(target_os = "ios")]
            _api.register_ios_plugin(init_plugin_native_tabbar)?;
            Ok(())
        })
        .build()
}
