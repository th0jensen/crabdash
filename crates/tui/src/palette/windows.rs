//! The Windows console has no color query reply on stdin; pills use reverse video.

pub fn query() -> Option<String> {
    None
}
