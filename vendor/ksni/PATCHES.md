Vendored from ksni 0.3.6, https://github.com/iovxw/ksni (Unlicense).

Crabdash adds the StatusNotifierItem `ProvideXdgActivationToken` D-Bus method
and a corresponding Tray callback. GNOME and KDE can supply a compositor token
before activating a tray menu item. This lets Crabdash restore its existing GPUI
Wayland window without a focus-stealing notification. All other library code is
unchanged. The manifest omits upstream examples and development dependencies.

Remove this local copy when upstream exposes the activation-token callback.
