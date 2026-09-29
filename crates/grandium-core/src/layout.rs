//! Where the launcher window goes on screen.

/// A rectangle in physical pixels, e.g. a monitor's work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Top-left corner for a launcher window of `width` × `height` pixels:
/// centered horizontally, with its top edge a quarter of the way down the
/// work area. The top edge stays put as results make the window taller.
/// Always keeps the window inside the work area where it fits.
pub fn launcher_origin(area: Rect, width: u32, height: u32) -> (i32, i32) {
    let free_x = i64::from(area.width) - i64::from(width);
    let free_y = i64::from(area.height) - i64::from(height);

    let x = i64::from(area.x) + (free_x / 2).max(0);
    let y = i64::from(area.y) + (i64::from(area.height) / 4).min(free_y).max(0);

    (clamp_i32(x), clamp_i32(y))
}

fn clamp_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };

    #[test]
    fn centers_horizontally_a_quarter_down() {
        assert_eq!(launcher_origin(FULL_HD, 680, 100), (620, 260));
    }

    #[test]
    fn respects_monitor_offset() {
        // A second monitor to the left of the primary one.
        let left = Rect {
            x: -2560,
            y: 0,
            width: 2560,
            height: 1400,
        };
        assert_eq!(launcher_origin(left, 1000, 120), (-1780, 350));
    }

    #[test]
    fn keeps_tall_window_on_screen() {
        // 900px tall: a quarter down (260) would overflow, so move it up.
        assert_eq!(launcher_origin(FULL_HD, 680, 900), (620, 140));
    }

    #[test]
    fn oversized_window_pins_to_top_left_of_area() {
        let small = Rect {
            x: 100,
            y: 50,
            width: 400,
            height: 300,
        };
        assert_eq!(launcher_origin(small, 680, 500), (100, 50));
    }
}
