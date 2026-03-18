//! Documentation for my own reference:
//! <https://leopard-adc.pepas.com/technotes/tn2006/tn2179.html>

#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2::{AnyThread, MainThreadMarker};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAboutPanelOptionApplicationIcon, NSAboutPanelOptionApplicationName,
    NSAboutPanelOptionApplicationVersion, NSApplication, NSImage,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSDictionary, NSString};

use crate::{
    APP_ICON_PATH, APP_LICENSE, APP_NAME, APP_VERSION, app_authors_display, short_git_commit_hash,
};

#[cfg(target_os = "macos")]
pub(crate) fn show_about_dialog() {
    let mtm = MainThreadMarker::new().expect("About panel must open on the main thread");
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

    if let Some(icon) = load_app_icon() {
        keys.push(unsafe { NSAboutPanelOptionApplicationIcon });
        values.push(icon);
    }

    let options: RetainedAboutOptions = NSDictionary::from_retained_objects(&keys, &values);

    unsafe {
        app.orderFrontStandardAboutPanelWithOptions(&options);
    }
}

#[cfg(target_os = "macos")]
type RetainedAboutOptions =
    objc2::rc::Retained<NSDictionary<objc2_app_kit::NSAboutPanelOptionKey, AnyObject>>;

#[cfg(target_os = "macos")]
fn load_app_icon() -> Option<objc2::rc::Retained<AnyObject>> {
    let path = NSString::from_str(APP_ICON_PATH);
    let icon = NSImage::initByReferencingFile(NSImage::alloc(), &path)?;
    Some(icon.into())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_about_dialog() {}
