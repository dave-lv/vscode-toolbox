//! The tray icon's image: the `</>` mark (scripts/make-tray-icon.swift). macOS tints a black template image
//! to fit the menu bar. Windows and Linux trays show the image as it is, so they get a white mark on a dark
//! taskbar and a black one on a light taskbar, drawn at the tray's size so resizing doesn't blur it.

#[cfg(target_os = "macos")]
pub fn image(_app: &tauri::AppHandle) -> tauri::image::Image<'static> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")).expect("bundled tray icon")
}

#[cfg(not(target_os = "macos"))]
pub use other::{image, refresh};

#[cfg(not(target_os = "macos"))]
mod other {
    use std::sync::Mutex;

    use tauri::AppHandle;
    use tauri::image::Image;

    use crate::panel::TRAY;

    /// The sizes drawn, in pixels: the tray's 16 px at 100%, 125%, 150% and 200% display scaling.
    const SIZES: [f64; 4] = [16.0, 20.0, 24.0, 32.0];
    const WHITE: [&[u8]; 4] = [
        include_bytes!("../icons/tray/white-16.png"),
        include_bytes!("../icons/tray/white-20.png"),
        include_bytes!("../icons/tray/white-24.png"),
        include_bytes!("../icons/tray/white-32.png"),
    ];
    const BLACK: [&[u8]; 4] = [
        include_bytes!("../icons/tray/black-16.png"),
        include_bytes!("../icons/tray/black-20.png"),
        include_bytes!("../icons/tray/black-24.png"),
        include_bytes!("../icons/tray/black-32.png"),
    ];

    /// The image shown: (white, index into SIZES).
    static SHOWN: Mutex<Option<(bool, usize)>> = Mutex::new(None);

    pub fn image(app: &AppHandle) -> Image<'static> {
        let choice = choose(app);
        *SHOWN.lock().unwrap() = Some(choice);
        decode(choice)
    }

    /// Switches the image when the taskbar's colour or the display scaling has changed.
    pub fn refresh(app: &AppHandle) {
        let choice = choose(app);
        if SHOWN.lock().unwrap().replace(choice) != Some(choice)
            && let Some(tray) = app.tray_by_id(TRAY)
        {
            let _ = tray.set_icon(Some(decode(choice)));
        }
    }

    fn decode((white, size): (bool, usize)) -> Image<'static> {
        Image::from_bytes(if white { WHITE[size] } else { BLACK[size] }).expect("bundled tray icon")
    }

    fn choose(app: &AppHandle) -> (bool, usize) {
        (dark_taskbar(app), size(app))
    }

    /// Windows draws the image at the small-icon size, which grows with the display scaling.
    #[cfg(windows)]
    fn size(app: &AppHandle) -> usize {
        let scale = app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor());
        SIZES.iter().position(|s| *s >= 16.0 * scale - 0.5).unwrap_or(SIZES.len() - 1)
    }

    /// Linux panels scale the image to their own height; the largest scales down best.
    #[cfg(not(windows))]
    fn size(_app: &AppHandle) -> usize {
        SIZES.len() - 1
    }

    /// Windows: Settings > Personalization > Colors > "Choose your default Windows mode" (the taskbar's,
    /// separate from the apps' mode). Before Windows 10 1903 there was no setting and the taskbar was dark.
    #[cfg(windows)]
    fn dark_taskbar(_app: &AppHandle) -> bool {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("SystemUsesLightTheme");
        let mut light = 0u32;
        let mut len = size_of::<u32>() as u32;
        // SAFETY: the strings are NUL-terminated and outlive the call; `light` holds the DWORD asked for.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut light).cast(),
                &mut len,
            )
        };
        status != 0 || light == 0
    }

    /// Linux: GNOME's top bar is dark whatever the theme; other desktops' panels follow the theme.
    #[cfg(not(windows))]
    fn dark_taskbar(app: &AppHandle) -> bool {
        use tauri::{Manager, Theme};
        let gnome = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d == "GNOME"));
        gnome
            || app.get_webview_window(crate::panel::PANEL).and_then(|w| w.theme().ok()) != Some(Theme::Light)
    }
}
