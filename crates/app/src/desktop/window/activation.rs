//! Apply a tray host's Wayland activation token to the existing GPUI surface.
use anyhow::{Context as _, Result};
use gpui::Window;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_registry, wl_surface},
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1;

#[derive(Default)]
struct Activation(Option<xdg_activation_v1::XdgActivationV1>);

impl Dispatch<wl_registry::WlRegistry, ()> for Activation {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            if interface == "xdg_activation_v1" {
                state.0 = Some(registry.bind(name, 1, qh, ()));
            }
        }
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for Activation {
    fn event(
        _: &mut Self,
        _: &xdg_activation_v1::XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

pub(crate) fn activate(window: &Window, token: &str) -> Result<bool> {
    let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) = (
        window.display_handle()?.as_raw(),
        HasWindowHandle::window_handle(window)?.as_raw(),
    ) else {
        return Ok(false);
    };
    // GPUI owns both handles. This temporary guest connection never closes its
    // display or destroys its surface, and all borrowed handles stay on the UI thread.
    let backend = unsafe {
        wayland_backend::client::Backend::from_foreign_display(display.display.as_ptr().cast())
    };
    let connection = Connection::from_backend(backend);
    let surface_id = unsafe {
        wayland_backend::client::ObjectId::from_ptr(
            wl_surface::WlSurface::interface(),
            surface.surface.as_ptr().cast(),
        )
    }?;
    let surface = wl_surface::WlSurface::from_id(&connection, surface_id)?;
    let mut queue = connection.new_event_queue::<Activation>();
    let registry = connection.display().get_registry(&queue.handle(), ());
    let mut state = Activation::default();
    let result = (|| {
        queue
            .roundtrip(&mut state)
            .context("Unable to discover Wayland activation support")?;
        let Some(activation) = state.0.as_ref() else {
            return Ok(false);
        };
        activation.activate(token.to_owned(), &surface);
        connection.flush().context("Unable to activate Crabdash")?;
        Ok(true)
    })();
    if let Some(activation) = state.0 {
        activation.destroy();
    }
    let _ = connection.backend().destroy_object(&registry.id());
    let _ = connection.flush();
    result
}
