use super::*;
use anyhow::{Context as _, Result};
use gpui::{Bounds, point, size};

fn fixture() -> Result<(TerminalState, Grid, Point<Pixels>)> {
    let terminal = TerminalState::new_interactive(20, 5, 100, 8, 20)?;
    let grid = Grid::new(
        Bounds::new(point(px(0.0), px(0.0)), size(px(180.0), px(126.0))),
        20,
        5,
        8.0,
        20.0,
    )
    .context("valid measured terminal grid")?;
    let position = point(px(30.0), px(46.0)); // Column 2, row 1, after 10/16px padding.
    Ok((terminal, grid, position))
}

#[test]
fn disabled_mouse_and_shift_leave_host_selection_in_control() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    assert!(
        terminal
            .mouse_press(MouseButton::Left, Modifiers::default(), position, grid)?
            .is_none()
    );
    terminal.feed(b"\x1b[?1002h\x1b[?1006h");
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    assert!(
        terminal
            .mouse_press(MouseButton::Left, shift, position, grid)?
            .is_none()
    );
    assert!(
        terminal
            .mouse_motion(
                Some(MouseButton::Left),
                Modifiers::default(),
                position,
                grid
            )?
            .is_none()
    );
    assert!(
        terminal
            .mouse_release(MouseButton::Left, Modifiers::default(), position, grid)?
            .is_none()
    );
    assert!(
        terminal
            .mouse_wheel(
                ScrollDelta::Lines(point(0.0, 2.0)),
                shift,
                position,
                grid,
                20.0
            )?
            .is_none()
    );
    assert!(terminal.take_pty_writes().is_empty());
    Ok(())
}

#[test]
fn sgr_press_release_modifiers_and_shift_after_press_are_reported() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?1000h\x1b[?1006h");
    let modifiers = Modifiers {
        control: true,
        alt: true,
        ..Modifiers::default()
    };
    assert_eq!(
        terminal.mouse_press(MouseButton::Left, modifiers, position, grid)?,
        Some(b"\x1b[<24;3;2M".to_vec())
    );
    assert!(
        terminal
            .mouse_motion(
                Some(MouseButton::Left),
                modifiers,
                point(px(50.0), px(46.0)),
                grid
            )?
            .is_some_and(|bytes| bytes.is_empty())
    );
    assert_eq!(
        terminal.mouse_release(
            MouseButton::Left,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            position,
            grid
        )?,
        Some(b"\x1b[<4;3;2m".to_vec())
    );
    assert!(!terminal.has_mouse_press());
    Ok(())
}

#[test]
fn application_drag_reports_changed_cells_and_release_outside() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?1002h\x1b[?1006h");
    terminal.mouse_press(MouseButton::Left, Modifiers::default(), position, grid)?;
    assert_eq!(
        terminal.mouse_motion(
            Some(MouseButton::Left),
            Modifiers::default(),
            position,
            grid
        )?,
        Some(Vec::new())
    );
    assert_eq!(
        terminal.mouse_motion(
            Some(MouseButton::Left),
            Modifiers::default(),
            point(px(31.0), px(47.0)),
            grid
        )?,
        Some(Vec::new())
    );
    assert_eq!(
        terminal.mouse_motion(
            Some(MouseButton::Left),
            Modifiers::default(),
            point(px(46.0), px(46.0)),
            grid
        )?,
        Some(b"\x1b[<32;5;2M".to_vec())
    );
    assert_eq!(
        terminal.mouse_release(
            MouseButton::Left,
            Modifiers::default(),
            point(px(-5.0), px(46.0)),
            grid
        )?,
        Some(b"\x1b[<0;1;2m".to_vec())
    );
    assert_eq!(
        terminal.mouse_motion(None, Modifiers::default(), position, grid)?,
        Some(Vec::new())
    );
    terminal.feed(b"\x1b[?1003h");
    assert_eq!(
        terminal.mouse_motion(None, Modifiers::default(), position, grid)?,
        Some(b"\x1b[<35;3;2M".to_vec())
    );
    Ok(())
}

#[test]
fn fractional_wheel_accumulates_and_supports_both_axes() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?1000h\x1b[?1006h");
    let delta = ScrollDelta::Pixels(point(px(0.0), px(10.0)));
    assert_eq!(
        terminal.mouse_wheel(delta, Modifiers::default(), position, grid, 20.0)?,
        Some(Vec::new())
    );
    assert_eq!(
        terminal.mouse_wheel(delta, Modifiers::default(), position, grid, 20.0)?,
        Some(b"\x1b[<64;3;2M".to_vec())
    );
    assert_eq!(
        terminal.mouse_wheel(
            ScrollDelta::Lines(point(-1.0, -1.0)),
            Modifiers::default(),
            position,
            grid,
            20.0
        )?,
        Some(b"\x1b[<65;3;2M\x1b[<67;3;2M".to_vec())
    );
    Ok(())
}

#[test]
fn x10_suppresses_release_and_sgr_pixels_uses_physical_cell_metrics() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?9h");
    assert_eq!(
        terminal.mouse_press(MouseButton::Left, Modifiers::default(), position, grid)?,
        Some(b"\x1b[M #\x22".to_vec())
    );
    assert_eq!(
        terminal.mouse_release(MouseButton::Left, Modifiers::default(), position, grid)?,
        Some(Vec::new())
    );
    terminal.feed(b"\x1b[?9l\x1b[?1000h\x1b[?1016h");
    terminal.resize(20, 5, 16, 40)?; // Retina: cells in GPUI retain their logical geometry.
    assert_eq!(
        terminal.mouse_press(MouseButton::Right, Modifiers::default(), position, grid)?,
        Some(b"\x1b[<2;40;60M".to_vec())
    );
    assert_eq!(terminal.cancel_mouse()?, b"\x1b[<2;40;60m");
    assert!(!terminal.has_mouse_press());
    assert!(terminal.cancel_mouse()?.is_empty());
    Ok(())
}

#[test]
fn wheel_input_is_bounded_and_invalid_values_do_not_poison_the_next_event() {
    let mut remainder = 10000.5;
    assert_eq!(wheel_steps(&mut remainder), 64);
    assert_eq!(remainder, 0.5);
    remainder = f32::NAN;
    assert_eq!(wheel_steps(&mut remainder), 0);
    assert_eq!(remainder, 0.0);
}

#[test]
fn overlapping_buttons_keep_each_press_paired_with_its_release() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?1002h\x1b[?1006h");
    for (button, expected) in [
        (MouseButton::Left, b"\x1b[<0;3;2M".as_slice()),
        (MouseButton::Right, b"\x1b[<2;3;2M".as_slice()),
    ] {
        assert_eq!(
            terminal.mouse_press(button, Modifiers::default(), position, grid)?,
            Some(expected.to_vec())
        );
    }
    assert_eq!(
        terminal.mouse_release(MouseButton::Left, Modifiers::default(), position, grid)?,
        Some(b"\x1b[<0;3;2m".to_vec())
    );
    assert!(terminal.has_mouse_press());
    assert_eq!(
        terminal.mouse_motion(
            Some(MouseButton::Right),
            Modifiers::default(),
            point(px(46.0), px(46.0)),
            grid
        )?,
        Some(b"\x1b[<34;5;2M".to_vec())
    );
    assert_eq!(
        terminal.mouse_release(MouseButton::Right, Modifiers::default(), position, grid)?,
        Some(b"\x1b[<2;3;2m".to_vec())
    );
    assert!(!terminal.has_mouse_press());
    for button in [MouseButton::Left, MouseButton::Right] {
        terminal.mouse_press(button, Modifiers::default(), position, grid)?;
    }
    assert_eq!(terminal.cancel_mouse()?, b"\x1b[<2;3;2m\x1b[<0;3;2m");
    assert!(!terminal.has_mouse_press());
    assert!(terminal.cancel_mouse()?.is_empty());
    Ok(())
}

#[test]
fn pixel_motion_reports_positions_within_the_same_cell() -> Result<()> {
    let (mut terminal, grid, position) = fixture()?;
    terminal.feed(b"\x1b[?1002h\x1b[?1016h");
    terminal.mouse_press(MouseButton::Left, Modifiers::default(), position, grid)?;
    assert_eq!(
        terminal.mouse_motion(
            Some(MouseButton::Left),
            Modifiers::default(),
            point(px(31.0), px(47.0)),
            grid
        )?,
        Some(b"\x1b[<32;21;31M".to_vec())
    );
    Ok(())
}
