//! Crabdash's opt-in native resize must remain ABI-compatible with this wrapper.
use libghostty_vt::{
    Terminal,
    error::Result,
    point::{Coordinate, Point},
};

fn rows(terminal: &Terminal<'_, '_>) -> Result<Vec<String>> {
    (0..terminal.rows()?)
        .map(|y| {
            let mut line = String::new();
            for x in 0..terminal.cols()? {
                let cell = terminal.grid_ref(Point::Active(Coordinate { x, y: y.into() }))?;
                let mut graphemes = ['\0'; 8];
                let len = cell.graphemes(&mut graphemes)?;
                if len == 0 {
                    line.push(' ');
                } else {
                    line.extend(&graphemes[..len]);
                }
            }
            Ok(line.trim_end().to_owned())
        })
        .collect()
}

#[test]
fn growth_prepends_blanks_and_keeps_cursor_attached_to_content() -> Result<()> {
    let mut terminal = Terminal::new(20, 5)?;
    terminal.vt_write(b"alpha\r\nbeta");
    terminal.resize_bottom(20, 8, 8, 20)?;
    assert_eq!(rows(&terminal)?, ["", "", "", "alpha", "beta", "", "", ""]);
    assert_eq!(terminal.cursor_y()?, 4);
    assert_eq!(terminal.width_px()?, 160);
    assert_eq!(terminal.height_px()?, 160);
    terminal.vt_write(b"!");
    assert_eq!(rows(&terminal)?[4], "beta!");
    Ok(())
}

#[test]
fn shrinking_then_growing_reveals_history_and_preserves_saved_cursor() -> Result<()> {
    let mut terminal = Terminal::new(20, 5)?;
    terminal.set_scrollback_max_lines(Some(200))?;
    terminal.vt_write(b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
    terminal.resize_bottom(20, 3, 8, 20)?;
    assert_eq!(rows(&terminal)?, ["three", "four", "five"]);
    terminal.resize_bottom(20, 5, 8, 20)?;
    assert_eq!(rows(&terminal)?, ["one", "two", "three", "four", "five"]);
    terminal.vt_write(b"\x1b7");
    terminal.resize_bottom(20, 8, 8, 20)?;
    terminal.vt_write(b"\x1b[H\x1b8!");
    assert_eq!(rows(&terminal)?[7], "five!");
    assert_eq!(terminal.cursor_y()?, 7);
    Ok(())
}

#[test]
fn changing_both_dimensions_preserves_reflow_and_pending_wrap() -> Result<()> {
    let mut terminal = Terminal::new(8, 3)?;
    terminal.vt_write(b"abcdefghijklmnop");
    terminal.resize_bottom(4, 6, 8, 20)?;
    assert_eq!(rows(&terminal)?.concat(), "abcdefghijklmnop");
    terminal.resize_bottom(8, 8, 8, 20)?;
    assert_eq!(rows(&terminal)?.concat(), "abcdefghijklmnop");
    terminal.vt_write(b"!");
    assert_eq!(rows(&terminal)?.concat(), "abcdefghijklmnop!");
    Ok(())
}

#[test]
fn alternate_screen_and_original_resize_keep_their_native_coordinates() -> Result<()> {
    let mut terminal = Terminal::new(20, 5)?;
    terminal.vt_write(b"primary\x1b[?1049h\x1b[Happlication");
    terminal.resize_bottom(20, 8, 8, 20)?;
    assert_eq!(rows(&terminal)?[0], "application");
    assert_eq!(terminal.cursor_y()?, 0);
    terminal.vt_write(b"\x1b[?1049l");
    assert_eq!(rows(&terminal)?[3], "primary");
    assert_eq!(terminal.cursor_y()?, 3);

    let mut standard = Terminal::new(20, 5)?;
    standard.vt_write(b"alpha\r\nbeta");
    standard.resize(20, 8, 8, 20)?;
    assert_eq!(rows(&standard)?[0], "alpha");
    assert_eq!(standard.cursor_y()?, 1);
    Ok(())
}
