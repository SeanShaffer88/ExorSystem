//! Desktop attachment boundary. Resize a zoomed window without unmaximizing it, so Windows keeps
//! its Restore button and the original normal placement.

use crate::activity_layout::{Rect, placement};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, GetWindowThreadProcessId, IsZoomed, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

fn outer_bounds(window: HWND) -> Result<Rect, String> {
    let mut rect = RECT::default();
    // SAFETY: rect is writable; callers supply a live window from this GUI thread.
    unsafe { GetWindowRect(window, &mut rect) }
        .map_err(|e| format!("could not read window bounds: {e}"))?;
    Ok(rect.into())
}

fn visible_bounds(window: HWND, outer: Rect) -> Rect {
    let mut frame = RECT::default();
    // SAFETY: DWMWA_EXTENDED_FRAME_BOUNDS writes one RECT; the output lives for the call.
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut frame).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    let frame: Rect = frame.into();
    if result.is_ok()
        && frame.width() > 0
        && frame.height() > 0
        && frame.left >= outer.left
        && frame.right <= outer.right
        && frame.top >= outer.top
        && frame.bottom <= outer.bottom
    {
        frame
    } else {
        outer
    }
}

fn set_bounds(window: HWND, current: Rect, target: Rect) -> Result<(), String> {
    if target == current {
        return Ok(());
    }
    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSENDCHANGING;
    if current.top == target.top && current.left == target.left {
        flags |= SWP_NOMOVE;
    }
    if current.width() == target.width() && current.height() == target.height() {
        flags |= SWP_NOSIZE;
    }
    // SAFETY: window belongs to this thread; Defer/follow is dispatched outside window callbacks.
    // We neither activate the window nor set its styles or overwrite WINDOWPLACEMENT.
    unsafe {
        SetWindowPos(
            window,
            None,
            target.left,
            target.top,
            target.width(),
            target.height(),
            flags,
        )
    }
    .map_err(|e| format!("could not position the activity attachment: {e}"))
}

/// Place two owned desktop windows in a physical monitor work area `(x, y, width, height)`.
/// Call on their GUI thread while the owner is not minimized. `reserved` remembers whether the
/// maximized owner needs its full work area back; it stays true if repositioning the log fails.
/// Visibility and lifetime are owned by the shell, not changed here.
pub fn place_activity_window(
    main: HWND,
    log: HWND,
    work: (i32, i32, u32, u32),
    scale: f64,
    open: bool,
    reserved: &mut bool,
) -> Result<(), String> {
    // SAFETY: both functions accept arbitrary handles/return 0 on invalid HWNDs. No pointers are passed.
    let on_window_thread = unsafe {
        let id = GetCurrentThreadId();
        GetWindowThreadProcessId(main, None) == id && GetWindowThreadProcessId(log, None) == id
    };
    if !on_window_thread || main == log {
        return Err("activity windows must belong to the same GUI thread".into());
    }
    let (x, y, width, height) = work;
    if width == 0 || height < 2 || !scale.is_finite() || scale <= 0.0 {
        return Err("no usable monitor work area for the activity log".into());
    }
    // SAFETY: the handle is the current thread's live main window.
    let maximized = unsafe { IsZoomed(main).as_bool() };
    if !(open || *reserved && maximized) {
        *reserved = false;
        return Ok(());
    }
    let work = Rect::new(x, y, width as i32, height as i32);
    let outer = outer_bounds(main)?;
    let frame = visible_bounds(main, outer);
    let (bounds, attachment) = placement(work, frame, scale, maximized, open);
    set_bounds(
        main,
        outer,
        Rect {
            left: bounds.left + outer.left - frame.left,
            top: bounds.top + outer.top - frame.top,
            right: bounds.right + outer.right - frame.right,
            bottom: bounds.bottom + outer.bottom - frame.bottom,
        },
    )?;
    *reserved = maximized && open;
    if let Some(bounds) = attachment {
        // The attachment has no decorations or shadow, so its outer bounds are its visible bounds.
        set_bounds(log, outer_bounds(log)?, bounds)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetWindowPlacement, SW_MAXIMIZE, SW_RESTORE,
        SW_SHOWNOACTIVATE, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WINDOWPLACEMENT,
        WS_OVERLAPPEDWINDOW, WS_POPUP,
    };
    use windows::core::w;

    struct TestWindow(HWND);
    impl TestWindow {
        fn new(style: WINDOW_STYLE, owner: Option<HWND>) -> Self {
            // SAFETY: STATIC is a system class; the literal strings live for the call. The owner
            // remains live on this thread until the owned window is destroyed.
            let window = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("ExorSystem geometry test"),
                    style,
                    100,
                    100,
                    480,
                    360,
                    owner,
                    None,
                    None,
                    None,
                )
            }
            .unwrap();
            Self(window)
        }

        fn normal_position(&self) -> RECT {
            let mut p = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            // SAFETY: the window is live and p is an initialized, correctly sized out parameter.
            unsafe { GetWindowPlacement(self.0, &mut p) }.unwrap();
            p.rcNormalPosition
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: this test owns the window and is still on its creation thread.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn windows_keeps_native_maximize_and_restore_while_reserving_space() {
        let main = TestWindow::new(WS_OVERLAPPEDWINDOW, None);
        let log = TestWindow::new(WS_POPUP, Some(main.0));
        // SAFETY: the test uses only its own live HWND, on the thread that created it.
        unsafe {
            let _ = ShowWindow(main.0, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(main.0, SW_MAXIMIZE);
        }
        let normal = main.normal_position();
        let maximized = outer_bounds(main.0).unwrap();
        let frame = visible_bounds(main.0, maximized);
        let work = (
            frame.left,
            frame.top,
            frame.width() as u32,
            frame.height() as u32,
        );
        let mut reserved = false;
        place_activity_window(main.0, log.0, work, 1.0, true, &mut reserved).unwrap();
        assert!(reserved);
        // SAFETY: both queries refer to the test's live window.
        assert!(unsafe { IsZoomed(main.0).as_bool() });
        assert_eq!(main.normal_position(), normal);
        let shrunk = outer_bounds(main.0).unwrap();
        let height = 220.min(frame.height() / 3);
        assert_eq!(shrunk.bottom, maximized.bottom - height);
        assert_eq!(
            outer_bounds(log.0).unwrap(),
            Rect {
                top: frame.bottom - height,
                ..frame
            }
        );

        place_activity_window(main.0, log.0, work, 1.0, false, &mut reserved).unwrap();
        assert!(!reserved);
        // SAFETY: the test's window is still alive; closing the log must not unmaximize it.
        assert!(unsafe { IsZoomed(main.0).as_bool() });
        assert_eq!(main.normal_position(), normal);
        assert_eq!(outer_bounds(main.0).unwrap(), maximized);
        // SAFETY: SW_RESTORE applies only to this test's window.
        let _ = unsafe { ShowWindow(main.0, SW_RESTORE) };
        // SAFETY: the window was restored, not destroyed.
        assert!(!unsafe { IsZoomed(main.0).as_bool() });
        assert_eq!(main.normal_position(), normal);
    }
}
