//! Gesture snapshots are installed immediately; Ghostty owns their tracked endpoints.
use crate::features::terminal::TerminalState;
use libghostty_vt::{
    Terminal,
    fmt::Format,
    screen::Screen,
    selection::{
        FormatOptions,
        gesture::{
            Autoscroll, AutoscrollTickEvent, Behavior, Behaviors, DragEvent, Gesture, PressEvent,
            ReleaseEvent,
        },
    },
    terminal::{Point, PointCoordinate},
};

pub(super) struct Stream {
    gesture: Gesture<'static>,
    press: PressEvent<'static>,
    drag: DragEvent<'static>,
    release: ReleaseEvent<'static>,
    tick: AutoscrollTickEvent<'static>,
    screen: Screen,
}

impl Stream {
    fn new(terminal: &Terminal<'_, '_>) -> anyhow::Result<Self> {
        Ok(Self {
            gesture: Gesture::new()?,
            press: PressEvent::new()?,
            drag: DragEvent::new()?,
            release: ReleaseEvent::new()?,
            tick: AutoscrollTickEvent::new()?,
            screen: terminal.active_screen()?,
        })
    }

    pub(in crate::features::terminal) fn reset(&mut self, terminal: &Terminal<'_, '_>) {
        self.gesture.reset(terminal);
    }
}

impl TerminalState {
    pub(in crate::features::terminal) fn selection_press(
        &mut self,
        cell: PointCoordinate,
        x: f64,
        y: f64,
        count: usize,
    ) -> anyhow::Result<()> {
        self.cancel_selection_gesture();
        if self.selection.stream.is_none() {
            self.selection.stream = Some(Stream::new(&self.terminal)?);
        }
        if let Some(stream) = &mut self.selection.stream {
            stream.screen = self.terminal.active_screen()?;
            let behavior = match count.saturating_sub(1) % 3 {
                1 => Behavior::Word,
                2 => Behavior::Line,
                _ => Behavior::Cell,
            };
            stream
                .press
                .set_behaviors(&Behaviors::new().with_single_click_behavior(behavior))?;
            stream.press.set_position(x, y)?;
            {
                let grid_ref = self.terminal.grid_ref(Point::Viewport(cell))?;
                let selection =
                    stream
                        .press
                        .apply(&mut stream.gesture, &self.terminal, grid_ref)?;
                self.terminal.set_selection(selection.as_ref())?;
            }
        }
        self.refresh_frame();
        Ok(())
    }

    pub(in crate::features::terminal) fn selection_drag(
        &mut self,
        cell: PointCoordinate,
        x: f64,
        y: f64,
        rectangle: bool,
        geometry: libghostty_vt::selection::gesture::Geometry,
    ) -> anyhow::Result<()> {
        if let Some(stream) = &mut self.selection.stream {
            stream.drag.set_position(x, y)?.set_rectangle(rectangle)?;
            {
                let grid_ref = self.terminal.grid_ref(Point::Viewport(cell))?;
                let selection =
                    stream
                        .drag
                        .apply(&mut stream.gesture, &self.terminal, grid_ref, geometry)?;
                self.terminal.set_selection(selection.as_ref())?;
            }
        }
        self.refresh_frame();
        Ok(())
    }

    pub(in crate::features::terminal) fn selection_tick(
        &mut self,
        cell: PointCoordinate,
        x: f64,
        y: f64,
        rectangle: bool,
        geometry: libghostty_vt::selection::gesture::Geometry,
    ) -> anyhow::Result<()> {
        if let Some(stream) = &mut self.selection.stream {
            stream.tick.set_position(x, y)?.set_rectangle(rectangle)?;
            let selection =
                stream
                    .tick
                    .apply(&mut stream.gesture, &self.terminal, cell, geometry)?;
            if let Some(selection) = selection {
                self.terminal.set_selection(Some(&selection))?;
            }
        }
        self.follow_output = self
            .terminal
            .scrollbar()
            .map(|bar| bar.offset.saturating_add(bar.len) >= bar.total)
            .unwrap_or(true);
        self.refresh_frame();
        Ok(())
    }

    pub(in crate::features::terminal) fn selection_release(
        &mut self,
        cell: Option<PointCoordinate>,
    ) -> anyhow::Result<()> {
        self.selection.held = None;
        self.selection.autoscroll = None;
        if let Some(stream) = &mut self.selection.stream {
            let grid_ref = cell
                .map(|cell| self.terminal.grid_ref(Point::Viewport(cell)))
                .transpose()?;
            stream
                .release
                .apply(&mut stream.gesture, &self.terminal, grid_ref)?;
        }
        Ok(())
    }

    pub(in crate::features::terminal) fn selection_autoscroll(&self) -> Autoscroll {
        self.selection
            .stream
            .as_ref()
            .and_then(|stream| stream.gesture.autoscroll(&self.terminal).ok())
            .unwrap_or(Autoscroll::None)
    }

    pub(in crate::features::terminal) fn cancel_selection_gesture(&mut self) {
        self.selection.held = None;
        self.selection.autoscroll = None;
        if let Some(stream) = &mut self.selection.stream {
            stream.reset(&self.terminal);
        }
    }

    pub(in crate::features::terminal) fn reconcile_selection_screen(&mut self) {
        if self
            .selection
            .stream
            .as_ref()
            .is_some_and(|stream| self.terminal.active_screen().ok() != Some(stream.screen))
        {
            self.cancel_selection_gesture();
        }
    }

    pub(in crate::features::terminal) fn clear_selection(&mut self) -> anyhow::Result<()> {
        self.cancel_selection_gesture();
        self.terminal.set_selection(None)?;
        self.refresh_frame();
        Ok(())
    }

    pub(in crate::features::terminal) fn has_selection(&self) -> bool {
        self.terminal.selection().ok().flatten().is_some()
    }

    pub(in crate::features::terminal) fn selected_text(&self) -> anyhow::Result<Option<String>> {
        let bytes = self.terminal.format_selection_alloc(
            None,
            FormatOptions::new()
                .with_emit_format(Format::Plain)
                .with_unwrap(true)
                .with_trim(true),
        )?;
        bytes
            .map(|bytes| String::from_utf8(bytes.to_vec()).map_err(Into::into))
            .transpose()
    }
}

impl Drop for TerminalState {
    fn drop(&mut self) {
        // Release gesture pins while their terminal is still alive.
        self.cancel_selection_gesture();
    }
}
