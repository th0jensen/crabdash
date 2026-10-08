//! Select an installed emoji fallback that the pinned Swash rasterizer can draw.
//!
//! Swash 0.2.10 supports COLRv0 and bitmap emoji, but not COLRv1 paint graphs.
//! Fedora's COLRv1-only Noto Color Emoji otherwise shapes successfully and paints
//! nothing. Keep the normal platform fallback policy unless that exact mismatch
//! is proven and a scalable monochrome Noto Emoji replacement is installed.

use cosmic_text::{Fallback, FontSystem, PlatformFallback, fontdb, ttf_parser};
use unicode_script::Script;

const COLOR_EMOJI: &str = "Noto Color Emoji";
const OUTLINE_EMOJI: &str = "Noto Emoji";

pub(super) fn with_supported_emoji(font_system: FontSystem) -> FontSystem {
    let fallback = SupportedEmojiFallback::for_database(font_system.db());
    if !fallback.replaces_color_emoji {
        return font_system;
    }
    // Preserve system locale, installed/bundled faces and generic family names.
    let (locale, database) = font_system.into_locale_and_db();
    FontSystem::new_with_locale_and_db_and_fallback(locale, database, fallback)
}

struct SupportedEmojiFallback {
    common: Vec<&'static str>,
    forbidden: Vec<&'static str>,
    replaces_color_emoji: bool,
}

impl SupportedEmojiFallback {
    fn for_database(database: &fontdb::Database) -> Self {
        let mut color_faces = Vec::new();
        let mut has_outline_emoji = false;
        // Inspect only the two relevant families, once at text-system creation.
        for info in database.faces() {
            let has_family = |name: &str| info.families.iter().any(|(family, _)| family == name);
            if has_family(COLOR_EMOJI) {
                let unsupported = database
                    .with_face_data(info.id, |bytes, index| {
                        ttf_parser::Face::parse(bytes, index)
                            .map(|face| {
                                let table =
                                    |tag| face.raw_face().table(ttf_parser::Tag::from_bytes(tag));
                                colrv1_only(
                                    table(b"COLR"),
                                    table(b"CBDT").is_some(),
                                    table(b"CBLC").is_some(),
                                    table(b"sbix").is_some(),
                                )
                            })
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                color_faces.push(unsupported);
            }
            if has_family(OUTLINE_EMOJI) && !has_outline_emoji {
                has_outline_emoji = database
                    .with_face_data(info.id, |bytes, index| {
                        ttf_parser::Face::parse(bytes, index)
                            .map(|face| {
                                let tables = face.tables();
                                (tables.glyf.is_some()
                                    || tables.cff.is_some()
                                    || tables.cff2.is_some())
                                    && face.glyph_index('\u{1f469}').is_some()
                                    && face.glyph_index('\u{1f4bb}').is_some()
                                    && face
                                        .raw_face()
                                        .table(ttf_parser::Tag::from_bytes(b"COLR"))
                                        .is_none()
                            })
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
            }
        }
        Self::for_faces(&color_faces, has_outline_emoji)
    }

    fn for_faces(color_faces: &[bool], has_outline_emoji: bool) -> Self {
        let replaces_color_emoji = has_outline_emoji
            && !color_faces.is_empty()
            && color_faces.iter().all(|unsupported| *unsupported);
        let mut common = PlatformFallback.common_fallback().to_vec();
        let mut forbidden = PlatformFallback.forbidden_fallback().to_vec();
        if replaces_color_emoji {
            for family in &mut common {
                if *family == COLOR_EMOJI {
                    *family = OUTLINE_EMOJI;
                }
            }
            // Cosmic Text's final catch-all stage could otherwise retry the
            // unsupported family when the replacement lacks a particular glyph.
            if !forbidden.contains(&COLOR_EMOJI) {
                forbidden.push(COLOR_EMOJI);
            }
        }
        Self {
            common,
            forbidden,
            replaces_color_emoji,
        }
    }
}

impl Fallback for SupportedEmojiFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &self.forbidden
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        PlatformFallback.script_fallback(script, locale)
    }
}

fn colrv1_only(colr: Option<&[u8]>, cbdt: bool, cblc: bool, sbix: bool) -> bool {
    if cbdt || cblc || sbix {
        return false;
    }
    let Some(colr) = colr else { return false };
    // A complete version-1 header contains the five additional Offset32 fields.
    if colr.len() < 34 || read_u16(colr, 0) != Some(1) || read_u16(colr, 2) != Some(0) {
        return false;
    }
    let Some(base_list) = read_u32(colr, 14).and_then(|offset| usize::try_from(offset).ok()) else {
        return false;
    };
    if base_list < 34 {
        return false;
    }
    let Some(count) = read_u32(colr, base_list).and_then(|count| usize::try_from(count).ok())
    else {
        return false;
    };
    let Some(records_end) = count
        .checked_mul(6)
        .and_then(|length| base_list.checked_add(4)?.checked_add(length))
    else {
        return false;
    };
    if count == 0 || records_end > colr.len() {
        return false;
    }
    // Truncated or invalid paint offsets are unknown, not proof of an unsupported
    // valid font. Supported color/bitmap faces must never be excluded by guesswork.
    (0..count).all(|index| {
        read_u32(colr, base_list + 4 + index * 6 + 2)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| base_list.checked_add(offset))
            .is_some_and(|paint| paint >= records_end && paint < colr.len())
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version_one_table() -> Vec<u8> {
        let mut table = vec![0; 49];
        table[0..2].copy_from_slice(&1_u16.to_be_bytes());
        table[14..18].copy_from_slice(&34_u32.to_be_bytes());
        table[34..38].copy_from_slice(&1_u32.to_be_bytes());
        table[40..44].copy_from_slice(&10_u32.to_be_bytes());
        // Complete PaintSolid: format, palette index 0 and opaque F2Dot14 alpha.
        table[44] = 2;
        table[47..49].copy_from_slice(&0x4000_u16.to_be_bytes());
        table
    }

    #[test]
    fn emoji_fallback_detects_only_unsupported_color_tables() {
        let mut table = version_one_table();
        assert!(colrv1_only(Some(&table), false, false, false));
        for (cbdt, cblc, sbix) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert!(!colrv1_only(Some(&table), cbdt, cblc, sbix));
        }
        table[2..4].copy_from_slice(&1_u16.to_be_bytes());
        assert!(!colrv1_only(Some(&table), false, false, false));
        table[0..2].copy_from_slice(&0_u16.to_be_bytes());
        assert!(!colrv1_only(Some(&table), false, false, false));
        assert!(!colrv1_only(None, false, false, false));
    }

    #[test]
    fn emoji_fallback_preserves_unknown_and_truncated_tables() {
        let table = version_one_table();
        // The detector bounds the header/list/root pointer, not the full paint graph.
        for length in 0..=44 {
            assert!(!colrv1_only(Some(&table[..length]), false, false, false));
        }
        for (field, value) in [
            (14, 0),
            (14, u32::MAX),
            (34, u32::MAX),
            (40, 0),
            (40, u32::MAX),
        ] {
            let mut malformed = table.clone();
            malformed[field..field + 4].copy_from_slice(&value.to_be_bytes());
            assert!(!colrv1_only(Some(&malformed), false, false, false));
        }
    }

    #[test]
    fn emoji_fallback_preserves_defaults_without_proven_replacement() {
        for (faces, outline) in [
            (&[][..], true),
            (&[true][..], false),
            (&[false][..], true),
            (&[true, false][..], true),
        ] {
            let fallback = SupportedEmojiFallback::for_faces(faces, outline);
            assert!(!fallback.replaces_color_emoji);
            assert_eq!(
                fallback.common_fallback(),
                PlatformFallback.common_fallback()
            );
            assert_eq!(
                fallback.forbidden_fallback(),
                PlatformFallback.forbidden_fallback()
            );
        }
    }

    #[test]
    fn emoji_fallback_replaces_only_emoji_and_delegates_script_policy() {
        let fallback = SupportedEmojiFallback::for_faces(&[true, true], true);
        assert!(fallback.replaces_color_emoji);
        let expected = PlatformFallback
            .common_fallback()
            .iter()
            .map(|family| {
                if *family == COLOR_EMOJI {
                    OUTLINE_EMOJI
                } else {
                    *family
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(fallback.common_fallback(), expected);
        assert!(fallback.forbidden_fallback().contains(&COLOR_EMOJI));
        for script in [
            Script::Common,
            Script::Latin,
            Script::Arabic,
            Script::Han,
            Script::Hangul,
        ] {
            for locale in ["en-US", "ja", "zh-HK", "ko"] {
                assert_eq!(
                    fallback.script_fallback(script, locale),
                    PlatformFallback.script_fallback(script, locale)
                );
            }
        }
    }
}
