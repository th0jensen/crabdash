//! Application mouse reporting. Shift keeps host selection and history scrolling available.
use super::{TerminalState, selection::Grid};
use gpui::{Modifiers, MouseButton, NavigationDirection, Pixels, Point, ScrollDelta, px};
use libghostty_vt::{
    Terminal,
    key::Mods,
    mouse::{Action, Button, Encoder, EncoderSize, Event, Position},
};

#[derive(Default)]
pub(super) struct State {
    encoder: Option<Encoder<'static>>,
    held: Vec<MouseButton>,
    last: Option<(Position, EncoderSize)>,
    wheel_x: f32,
    wheel_y: f32,
}

fn button(button: MouseButton) -> Button {
    match button {
        MouseButton::Left => Button::Left,
        MouseButton::Middle => Button::Middle,
        MouseButton::Right => Button::Right,
        MouseButton::Navigate(NavigationDirection::Back) => Button::Eight,
        MouseButton::Navigate(NavigationDirection::Forward) => Button::Nine,
    }
}

fn modifiers(modifiers: Modifiers) -> Mods {
    let mut result = Mods::empty();
    result.set(Mods::SHIFT, modifiers.shift);
    result.set(Mods::ALT, modifiers.alt);
    result.set(Mods::CTRL, modifiers.control);
    result.set(Mods::SUPER, modifiers.platform);
    result
}

impl State {
    fn encode(
        &mut self,
        terminal: &Terminal<'_, '_>,
        action: Action,
        button: Option<Button>,
        modifiers: Modifiers,
        position: Position,
        size: EncoderSize,
        pressed: bool,
    ) -> anyhow::Result<Vec<u8>> {
        if self.encoder.is_none() {
            let mut encoder = Encoder::new()?;
            encoder.set_track_last_cell(true);
            self.encoder = Some(encoder);
        }
        let Some(encoder) = &mut self.encoder else {
            return Ok(Vec::new());
        };
        encoder
            .set_options_from_terminal(terminal)
            .set_size(size)
            .set_any_button_pressed(pressed);
        let mut event = Event::new()?;
        event
            .set_action(action)
            .set_button(button)
            .set_mods(self::modifiers(modifiers))
            .set_position(position);
        let mut bytes = Vec::with_capacity(32);
        encoder.encode_to_vec(&event, &mut bytes)?;
        self.last = Some((position, size));
        Ok(bytes)
    }

    fn reset(&mut self) {
        self.held.clear();
        self.last = None;
        self.wheel_x = 0.0;
        self.wheel_y = 0.0;
        if let Some(encoder) = &mut self.encoder {
            encoder.reset();
        }
    }
}

impl TerminalState {
    pub(super) fn has_mouse_press(&self) -> bool {
        !self.mouse.held.is_empty()
    }

    fn mouse_geometry(
        &self,
        grid: Grid,
        position: Point<Pixels>,
    ) -> anyhow::Result<(Position, EncoderSize)> {
        let columns = u32::from(self.terminal.cols()?).max(1);
        let rows = u32::from(self.terminal.rows()?).max(1);
        let cell_width = (self.terminal.width_px()? / columns).max(1);
        let cell_height = (self.terminal.height_px()? / rows).max(1);
        let (x, y) = grid.mouse_position(position);
        Ok((
            Position {
                x: x * cell_width as f32,
                y: y * cell_height as f32,
            },
            EncoderSize {
                screen_width: columns * cell_width,
                screen_height: rows * cell_height,
                cell_width,
                cell_height,
                padding_top: 0,
                padding_bottom: 0,
                padding_left: 0,
                padding_right: 0,
            },
        ))
    }

    /// None leaves the event to text selection; Some consumes it even if the protocol suppresses it.
    pub(super) fn mouse_press(
        &mut self,
        button: MouseButton,
        modifiers: Modifiers,
        position: Point<Pixels>,
        grid: Grid,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if modifiers.shift || !self.terminal.is_mouse_tracking()? {
            return Ok(None);
        }
        let (position, size) = self.mouse_geometry(grid, position)?;
        let bytes = self.mouse.encode(
            &self.terminal,
            Action::Press,
            Some(self::button(button)),
            modifiers,
            position,
            size,
            true,
        )?;
        self.clear_selection()?;
        if !self.mouse.held.contains(&button) {
            self.mouse.held.push(button);
        }
        Ok(Some(bytes))
    }

    pub(super) fn mouse_motion(
        &mut self,
        pressed: Option<MouseButton>,
        modifiers: Modifiers,
        position: Point<Pixels>,
        grid: Grid,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if !self.mouse.held.is_empty() && pressed.is_none() {
            return self.cancel_mouse().map(Some);
        }
        if modifiers.shift || !self.terminal.is_mouse_tracking()? {
            return Ok(None);
        }
        // A text-selection gesture begun with Shift must never become an application drag.
        if pressed.is_some_and(|button| !self.mouse.held.contains(&button)) {
            return Ok(None);
        }
        let (position, size) = self.mouse_geometry(grid, position)?;
        self.mouse
            .encode(
                &self.terminal,
                Action::Motion,
                pressed.map(button),
                modifiers,
                position,
                size,
                pressed.is_some(),
            )
            .map(Some)
    }

    pub(super) fn mouse_release(
        &mut self,
        button: MouseButton,
        modifiers: Modifiers,
        position: Point<Pixels>,
        grid: Grid,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(index) = self.mouse.held.iter().position(|held| *held == button) else {
            return Ok(None);
        };
        // Release a reported press even when Shift was pressed while dragging.
        self.mouse.held.remove(index);
        let (position, size) = self.mouse_geometry(grid, position)?;
        self.mouse
            .encode(
                &self.terminal,
                Action::Release,
                Some(self::button(button)),
                modifiers,
                position,
                size,
                !self.mouse.held.is_empty(),
            )
            .map(Some)
    }

    pub(super) fn mouse_wheel(
        &mut self,
        delta: ScrollDelta,
        modifiers: Modifiers,
        position: Point<Pixels>,
        grid: Grid,
        line_height: f32,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if modifiers.shift || !self.terminal.is_mouse_tracking()? {
            self.mouse.wheel_x = 0.0;
            self.mouse.wheel_y = 0.0;
            return Ok(None);
        }
        let delta = delta.pixel_delta(px(line_height));
        self.mouse.wheel_x += f32::from(delta.x) / line_height;
        self.mouse.wheel_y += f32::from(delta.y) / line_height;
        let x = wheel_steps(&mut self.mouse.wheel_x);
        let y = wheel_steps(&mut self.mouse.wheel_y);
        let (position, size) = self.mouse_geometry(grid, position)?;
        let mut bytes = Vec::new();
        for (steps, positive, negative) in [
            (y, Button::Four, Button::Five),
            (x, Button::Six, Button::Seven),
        ] {
            for _ in 0..steps.unsigned_abs() {
                bytes.extend(self.mouse.encode(
                    &self.terminal,
                    Action::Press,
                    Some(if steps > 0 { positive } else { negative }),
                    modifiers,
                    position,
                    size,
                    !self.mouse.held.is_empty(),
                )?);
            }
        }
        Ok(Some(bytes))
    }

    /// Pair a press with a release when its pane loses focus or is hidden.
    pub(super) fn cancel_mouse(&mut self) -> anyhow::Result<Vec<u8>> {
        let result = (|| {
            let mut bytes = Vec::new();
            if let Some((position, size)) = self.mouse.last {
                while let Some(held) = self.mouse.held.pop() {
                    bytes.extend(self.mouse.encode(
                        &self.terminal,
                        Action::Release,
                        Some(button(held)),
                        Modifiers::default(),
                        position,
                        size,
                        !self.mouse.held.is_empty(),
                    )?);
                }
            }
            Ok(bytes)
        })();
        self.mouse.reset();
        result
    }
}

fn wheel_steps(remainder: &mut f32) -> i32 {
    if !remainder.is_finite() {
        *remainder = 0.0;
        return 0;
    }
    let steps = remainder.trunc();
    *remainder -= steps;
    steps.clamp(-64.0, 64.0) as i32
}

#[cfg(test)]
mod tests;
