//! Physical-pixel layout for an attached activity window. Kept separate from Win32 so the
//! work-area and scaling rules can be tested without a desktop.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

impl Rect {
    pub(crate) fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    pub(crate) fn width(self) -> i32 {
        self.right - self.left
    }

    pub(crate) fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Returns visible bounds. Keep a normal window's width/height where possible, and allow it to
/// cross a horizontal monitor seam. Only the part of the log on the owner's monitor is shown.
/// A maximized window instead shares the whole work area with the log.
pub(crate) fn placement(
    work: Rect,
    current: Rect,
    scale: f64,
    maximized: bool,
    open: bool,
) -> (Rect, Option<Rect>) {
    if !open {
        return (if maximized { work } else { current }, None);
    }

    // Don't let a high-DPI or short display spend more than a third of its work area on log lines.
    let preferred = ((220.0 * scale).round() as i32).clamp(1, (work.height() / 3).max(1));
    let minimum = ((112.0 * scale).round() as i32).clamp(1, preferred);
    let height = if maximized {
        preferred
    } else {
        (work.height() - current.height()).clamp(minimum, preferred)
    };
    let end = work.bottom - height;
    let main = if maximized {
        Rect {
            bottom: end,
            ..work
        }
    } else {
        let main_height = current.height().min(end - work.top);
        let top = current.top.clamp(work.top, end - main_height);
        // Reposition horizontally only if the window has no intersection at all, for example
        // after disconnecting a monitor. Otherwise don't pin it to one side of a monitor seam.
        let left = if current.right <= work.left || current.left >= work.right {
            current
                .left
                .clamp(work.left, work.right - current.width().min(work.width()))
        } else {
            current.left
        };
        Rect::new(left, top, current.width(), main_height)
    };
    let log = Rect {
        left: main.left.max(work.left),
        top: main.bottom,
        right: main.right.min(work.right),
        bottom: main.bottom + height,
    };
    (main, Some(log))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximized_attachment_uses_and_reclaims_the_monitor_work_area() {
        // Secondary monitor to the left and above primary; taskbar has consumed the top edge.
        let work = Rect::new(-1920, -960, 1920, 960);
        let (main, log) = placement(work, work, 1.5, true, true);
        assert_eq!(main, Rect::new(-1920, -960, 1920, 640));
        assert_eq!(log, Some(Rect::new(-1920, -320, 1920, 320)));
        assert_eq!(placement(work, main, 1.5, true, false), (work, None));
        assert_eq!(placement(work, main, 1.5, true, true), (main, log));
    }

    #[test]
    fn normal_window_keeps_its_size_and_moves_only_if_needed() {
        let work = Rect::new(40, 0, 1240, 760);
        let normal = Rect::new(100, 20, 850, 450);
        let (main, log) = placement(work, normal, 1.0, false, true);
        assert_eq!(main, normal);
        assert_eq!(log, Some(Rect::new(100, 470, 850, 220)));

        let low = Rect::new(100, 238, 850, 500);
        let (main, log) = placement(work, low, 1.0, false, true);
        assert_eq!(main, Rect::new(100, 40, 850, 500));
        assert_eq!(log, Some(Rect::new(100, 540, 850, 220)));
        assert_eq!(placement(work, main, 1.0, false, false), (main, None));
    }

    #[test]
    fn normal_attachment_can_cross_a_monitor_seam_and_fit_a_short_display() {
        let work = Rect::new(-1000, 0, 1000, 620);
        let normal = Rect::new(-580, 20, 800, 610);
        let (main, log) = placement(work, normal, 1.5, false, true);
        assert_eq!(main, Rect::new(-580, 0, 800, 452));
        assert_eq!(log, Some(Rect::new(-580, 452, 580, 168)));
        assert_eq!(placement(work, main, 1.5, false, true), (main, log));

        let right = Rect::new(0, 0, 1600, 860);
        let moved = Rect::new(-280, 30, 800, 452);
        let (main, log) = placement(right, moved, 1.0, false, true);
        assert_eq!(main, moved);
        assert_eq!(log, Some(Rect::new(0, 482, 520, 220)));
    }
}
