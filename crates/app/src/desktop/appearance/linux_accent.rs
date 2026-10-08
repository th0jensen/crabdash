//! Read the desktop's accent through the session portal, outside the UI thread.
use ashpd::desktop::settings::{ACCENT_COLOR_SCHEME_KEY, APPEARANCE_NAMESPACE, Settings};
use gpui::App;
use smol::{
    channel::{Receiver, Sender},
    stream::StreamExt,
};

pub(super) fn start(cx: &mut App) -> Option<Receiver<Option<u32>>> {
    let (sender, receiver) = smol::channel::unbounded();
    cx.background_executor()
        .spawn(async move {
            if let Err(error) = watch(&sender).await {
                tracing::debug!(%error, "Desktop accent portal is unavailable");
                let _ = sender.try_send(None);
            }
        })
        .detach();
    Some(receiver)
}

async fn watch(sender: &Sender<Option<u32>>) -> ashpd::Result<()> {
    let settings = Settings::new().await?;
    // Subscribe first so an accent change during the initial read is queued.
    // The typed stream retains decode errors, unlike the convenience accent
    // stream, so a malformed update can clear a previously cached color.
    let mut changes = settings
        .receive_setting_changed_with_args::<(f64, f64, f64)>(
            APPEARANCE_NAMESPACE,
            ACCENT_COLOR_SCHEME_KEY,
        )
        .await?;
    let initial = settings
        .read::<(f64, f64, f64)>(APPEARANCE_NAMESPACE, ACCENT_COLOR_SCHEME_KEY)
        .await
        .ok()
        .and_then(|(red, green, blue)| super::accent::rgb_from_components([red, green, blue]));
    if sender.try_send(initial).is_err() {
        return Ok(());
    }

    while let Some(change) = changes.next().await {
        let accent = match change {
            Ok((red, green, blue)) => super::accent::rgb_from_components([red, green, blue]),
            Err(error) => {
                tracing::debug!(%error, "Desktop accent portal returned an invalid color");
                None
            }
        };
        if sender.try_send(accent).is_err() {
            return Ok(());
        }
    }
    // A disconnected signal stream must not leave an old desktop color cached.
    let _ = sender.try_send(None);
    Ok(())
}
