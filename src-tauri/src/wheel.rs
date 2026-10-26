//! Which slices a wheel shows and which slice a point falls on. The overlay draws exactly the
//! list computed here, so what is highlighted is what runs.

use crate::config::{WheelConfig, WheelItem};
use crate::converter::{category_of, ext_of};
use std::path::Path;

/// Wheel geometry in logical pixels (must match overlay.html).
pub const CENTER: f64 = 180.0;
pub const R_INNER: f64 = 80.0;
pub const R_OUTER: f64 = 146.0;
/// Drops slightly outside the ring still count, so the edge is easy to hit.
pub const R_HIT: f64 = R_OUTER + 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// The center hole: cancel.
    Hole,
    Ring,
    Outside,
}

pub fn zone(lx: f64, ly: f64) -> Zone {
    let dist = (lx - CENTER).hypot(ly - CENTER);
    if dist < R_INNER {
        Zone::Hole
    } else if dist <= R_HIT {
        Zone::Ring
    } else {
        Zone::Outside
    }
}

/// Category of a selection: "folder" if anything is a folder, else the first file's type.
pub fn category_for(paths: &[String]) -> &'static str {
    if paths.iter().any(|p| Path::new(p).is_dir()) {
        return "folder";
    }
    paths.first().map(|p| category_of(p)).unwrap_or("unknown")
}

/// Extension shared by the whole selection, if any (used to hide "convert to the same format").
pub fn common_ext(paths: &[String]) -> Option<String> {
    let mut exts = paths.iter().map(|p| ext_of(Path::new(p)));
    let first = exts.next()?;
    if first.is_empty() || !exts.all(|e| e == first) {
        return None;
    }
    Some(first)
}

pub fn items_for(conf: &WheelConfig, mode: &str, paths: &[String]) -> Vec<WheelItem> {
    let category = category_for(paths);
    let count = paths.len();
    let list: Vec<WheelItem> = match mode {
        "actions" => conf.actions.clone(),
        "tools" => conf.tools.get(category).cloned().unwrap_or_default(),
        _ if category == "folder" => Vec::new(),
        _ => conf.convert.get(category).cloned().unwrap_or_default(),
    };
    let same = common_ext(paths).map(|e| format!("convert:{e}"));
    list.into_iter()
        .filter(|item| !item.multi.unwrap_or(false) || count > 1)
        .filter(|item| Some(&item.id) != same.as_ref())
        .collect()
}

/// Index of the slice under a point, for `count` slices starting at 12 o'clock going clockwise.
pub fn slice_at(count: usize, lx: f64, ly: f64) -> Option<usize> {
    if count == 0 || zone(lx, ly) != Zone::Ring {
        return None;
    }
    let step = 360.0 / count as f64;
    let angle = ((ly - CENTER).atan2(lx - CENTER).to_degrees() + 90.0 + step / 2.0).rem_euclid(360.0);
    Some((angle / step).floor() as usize % count)
}

pub fn resolve(items: &[WheelItem], lx: f64, ly: f64) -> Option<String> {
    slice_at(items.len(), lx, ly).map(|i| items[i].id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::get_wheel_config;

    fn ids(items: &[WheelItem]) -> Vec<&str> {
        items.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn same_format_is_hidden() {
        let conf = get_wheel_config();
        let png = ids(&items_for(&conf, "convert", &["a.PNG".into()])).join(",");
        assert!(!png.contains("convert:png"), "{png}");
        assert!(png.contains("convert:jpg"));
        let jpeg = ids(&items_for(&conf, "convert", &["a.jpeg".into()])).join(",");
        assert!(!jpeg.contains("convert:jpg"), "{jpeg}");
        let docx = ids(&items_for(&conf, "convert", &["r.docx".into()])).join(",");
        assert!(!docx.contains("convert:docx") && docx.contains("convert:pdf"), "{docx}");
        // Mixed selections keep every target.
        let mixed = ids(&items_for(&conf, "convert", &["a.png".into(), "b.jpg".into()])).join(",");
        assert!(mixed.contains("convert:png") && mixed.contains("convert:jpg"));
    }

    #[test]
    fn folders_get_no_conversions_but_keep_actions() {
        let conf = get_wheel_config();
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        assert!(items_for(&conf, "convert", std::slice::from_ref(&dir)).is_empty());
        assert_eq!(items_for(&conf, "actions", &[dir]).len(), conf.actions.len());
    }

    #[test]
    fn multi_tools_need_two_files() {
        let conf = get_wheel_config();
        assert!(!ids(&items_for(&conf, "tools", &["a.pdf".into()])).contains(&"tool:pdf.merge"));
        assert!(ids(&items_for(&conf, "tools", &["a.pdf".into(), "b.pdf".into()])).contains(&"tool:pdf.merge"));
    }

    #[test]
    fn slices_go_clockwise_from_top() {
        // 4 slices: top, right, bottom, left
        assert_eq!(slice_at(4, CENTER, CENTER - 110.0), Some(0));
        assert_eq!(slice_at(4, CENTER + 110.0, CENTER), Some(1));
        assert_eq!(slice_at(4, CENTER, CENTER + 110.0), Some(2));
        assert_eq!(slice_at(4, CENTER - 110.0, CENTER), Some(3));
        assert_eq!(slice_at(4, CENTER + 10.0, CENTER), None); // hole
        assert_eq!(slice_at(4, CENTER + 170.0, CENTER), None); // outside
        assert_eq!(slice_at(4, CENTER + 160.0, CENTER), Some(1)); // forgiving edge
    }

    /// Writes hit-test vectors for the overlay UI test (tests/ui-e2e.mjs) when
    /// SHIFTWHEEL_WHEEL_VECTORS names an output file, so the page is checked against this code.
    #[test]
    fn export_hit_vectors_for_ui_test() {
        let Some(path) = std::env::var_os("SHIFTWHEEL_WHEEL_VECTORS") else { return };
        let mut cases = Vec::new();
        for count in 1..=10usize {
            let mut y = 0.0;
            while y <= 380.0 {
                let mut x = 0.0;
                while x <= 360.0 {
                    let expect = match zone(x, y) {
                        Zone::Hole => serde_json::json!("hole"),
                        _ => slice_at(count, x, y).map_or(serde_json::Value::Null, |i| serde_json::json!(i)),
                    };
                    cases.push(serde_json::json!({ "count": count, "x": x, "y": y, "expect": expect }));
                    x += 7.0;
                }
                y += 7.0;
            }
        }
        std::fs::write(path, serde_json::to_string(&cases).unwrap()).unwrap();
    }

    #[test]
    fn every_configured_item_is_runnable() {
        let conf = get_wheel_config();
        let all = conf.convert.values().chain(conf.tools.values()).flatten().chain(conf.actions.iter());
        for item in all {
            let (kind, value) = item.id.split_once(':').unwrap();
            match kind {
                "convert" => assert!(!value.is_empty()),
                "tool" => assert!(crate::jobs::known_tool(value), "no implementation for {}", item.id),
                "action" => assert!(crate::jobs::known_action(value), "no implementation for {}", item.id),
                _ => panic!("bad id {}", item.id),
            }
        }
    }
}
