//! Documentation for my own reference:
//! <https://leopard-adc.pepas.com/technotes/tn2006/tn2179.html>

use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionApplicationName,
    NSAboutPanelOptionApplicationVersion, NSApplication, NSImage,
};
use objc2_foundation::{NSBundle, NSDictionary, NSString};

use crate::APP_ICON_PATH;
use crate::{APP_LICENSE, APP_NAME, APP_VERSION, app_authors_display, short_git_commit_hash};

pub(crate) fn show_about_dialog(_window: &mut gpui::Window, _cx: &mut gpui::App) {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("The native About panel requires the main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);

    let app_name = NSString::from_str(APP_NAME);
    let app_version = NSString::from_str(&format!(
        "Version: {APP_VERSION} ({})\n\n© {} ({})",
        short_git_commit_hash(),
        app_authors_display(),
        APP_LICENSE
    ));

    let mut keys = unsafe {
        vec![
            NSAboutPanelOptionApplicationName,
            NSAboutPanelOptionApplicationVersion,
        ]
    };
    let mut values: Vec<_> = vec![app_name.into(), app_version.into()];

    if let Some(icon) = load_app_icon(&app) {
        keys.push(unsafe { NSAboutPanelOptionApplicationIcon });
        values.push(icon);
    }

    let options: RetainedAboutOptions = NSDictionary::from_retained_objects(&keys, &values);

    unsafe {
        app.orderFrontStandardAboutPanelWithOptions(&options);
    }
}

type RetainedAboutOptions =
    objc2::rc::Retained<NSDictionary<objc2_app_kit::NSAboutPanelOptionKey, AnyObject>>;

fn load_app_icon(app: &NSApplication) -> Option<objc2::rc::Retained<AnyObject>> {
    // Installed bundles provide their native icon, including macOS appearance
    // variants. A build-machine source path is only useful for developer runs.
    if NSBundle::mainBundle()
        .bundlePath()
        .to_string()
        .to_ascii_lowercase()
        .ends_with(".app")
    {
        return app
            .applicationIconImage()
            .filter(|icon| icon.isValid())
            .map(Into::into);
    }
    let path = NSString::from_str(APP_ICON_PATH);
    NSImage::initByReferencingFile(NSImage::alloc(), &path)
        .filter(|icon| icon.isValid())
        .or_else(|| app.applicationIconImage().filter(|icon| icon.isValid()))
        .map(Into::into)
}
