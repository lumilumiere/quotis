//! Where the overlay window goes. Pure geometry in physical pixels, so it is testable
//! without a display: the desktop shell supplies the monitor and applies the result.

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Gap between the top of the work area and a floating island, in logical pixels.
const ISLAND_MARGIN: f64 = 10.0;

/// Places a window of logical size `size` on a monitor.
///
/// `work` is the monitor's work area (the screen minus taskbar, menu bar and, on a MacBook,
/// the camera housing strip). The notch hangs from the top centre of it. The island keeps
/// the top-centre `anchor` the user dragged it to, as long as that point is on this monitor.
/// The result always lies inside the work area when it fits.
pub fn place(work: Rect, scale: f64, size: (f64, f64), notch: bool, anchor: Option<(i32, i32)>) -> Rect {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let w = ((size.0 * scale).round() as i32).clamp(1, work.w.max(1));
    let h = ((size.1 * scale).round() as i32).clamp(1, work.h.max(1));
    let default = (work.x + work.w / 2, work.y + if notch { 0 } else { (ISLAND_MARGIN * scale).round() as i32 });
    let (cx, top) = match anchor {
        Some((x, y)) if !notch && work.contains(x, y) => (x, y),
        _ => default,
    };
    let x = (cx - w / 2).clamp(work.x, (work.x + work.w - w).max(work.x));
    let y = top.clamp(work.y, (work.y + work.h - h).max(work.y));
    Rect { x, y, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FHD: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn notch_hangs_from_the_top_centre_and_ignores_the_anchor() {
        assert_eq!(place(FHD, 1.0, (200.0, 32.0), true, None), Rect { x: 860, y: 0, w: 200, h: 32 });
        assert_eq!(place(FHD, 1.0, (200.0, 32.0), true, Some((100, 500))), Rect { x: 860, y: 0, w: 200, h: 32 });
    }

    #[test]
    fn notch_sits_below_a_top_taskbar_or_menu_bar() {
        // macOS: the work area starts below the menu bar, which is also below any camera notch.
        let mac = Rect { x: 0, y: 74, w: 3024, h: 1890 };
        assert_eq!(place(mac, 2.0, (200.0, 32.0), true, None), Rect { x: 1312, y: 74, w: 400, h: 64 });
        // Windows with the taskbar docked on top.
        let top_bar = Rect { x: 0, y: 48, w: 1920, h: 1032 };
        assert_eq!(place(top_bar, 1.0, (200.0, 32.0), true, None).y, 48);
    }

    #[test]
    fn island_defaults_to_top_centre_with_a_margin_and_scales() {
        assert_eq!(place(FHD, 1.0, (200.0, 32.0), false, None), Rect { x: 860, y: 10, w: 200, h: 32 });
        assert_eq!(place(FHD, 1.5, (200.0, 32.0), false, None), Rect { x: 810, y: 15, w: 300, h: 48 });
    }

    #[test]
    fn island_keeps_its_anchor_when_expanding_and_stays_on_screen() {
        let collapsed = place(FHD, 1.0, (200.0, 32.0), false, Some((500, 300)));
        let expanded = place(FHD, 1.0, (300.0, 400.0), false, Some((500, 300)));
        assert_eq!((collapsed.x + collapsed.w / 2, collapsed.y), (500, 300));
        assert_eq!((expanded.x + expanded.w / 2, expanded.y), (500, 300), "grows around the same top-centre");
        // Near an edge the window is pushed back inside the work area.
        assert_eq!(place(FHD, 1.0, (300.0, 400.0), false, Some((1900, 1000))), Rect { x: 1620, y: 640, w: 300, h: 400 });
        assert_eq!(place(FHD, 1.0, (300.0, 400.0), false, Some((5, 5))).x, 0);
    }

    #[test]
    fn second_monitor_with_negative_origin() {
        let left = Rect { x: -2560, y: -200, w: 2560, h: 1400 };
        assert_eq!(place(left, 1.25, (200.0, 32.0), true, None), Rect { x: -1405, y: -200, w: 250, h: 40 });
        let r = place(left, 1.25, (200.0, 32.0), false, Some((-100, 0)));
        assert_eq!((r.x, r.y), (-250, 0));
    }

    #[test]
    fn an_anchor_on_an_unplugged_monitor_falls_back_to_the_default() {
        assert_eq!(place(FHD, 1.0, (200.0, 32.0), false, Some((-3000, 400))), place(FHD, 1.0, (200.0, 32.0), false, None));
    }

    #[test]
    fn degenerate_inputs_never_panic_or_escape() {
        let tiny = Rect { x: 0, y: 0, w: 100, h: 20 };
        assert_eq!(place(tiny, 1.0, (300.0, 400.0), false, None), Rect { x: 0, y: 0, w: 100, h: 20 });
        let r = place(FHD, f64::NAN, (200.0, 32.0), true, None);
        assert_eq!((r.w, r.h), (200, 32));
        let zero = place(Rect { x: 0, y: 0, w: 0, h: 0 }, 0.0, (0.0, -5.0), false, Some((0, 0)));
        assert_eq!((zero.w, zero.h), (1, 1));
    }
}
