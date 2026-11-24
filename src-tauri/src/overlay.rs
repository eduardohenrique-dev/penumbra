//! Camada escura por monitor: janelas Win32 nativas (sem WebView, custo ~0),
//! pretas, semitransparentes, sempre no topo e click-through.
//! Todas as janelas vivem numa thread própria com loop de mensagens; o resto
//! do app conversa com ela por canal.

use crate::monitors::MonitorInfo;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, IsWindowVisible,
    PeekMessageW, PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos,
    ShowWindow, TranslateMessage, HWND_TOPMOST, LWA_ALPHA, MSG, PM_NOREMOVE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_APP, WM_USER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

enum Cmd {
    Apply { id: String, x: i32, y: i32, w: u32, h: u32, alpha: u8 },
    Remove(String),
    RemoveAll,
    Reassert,
    Quit,
}

pub struct Overlays {
    tx: Sender<Cmd>,
    thread_id: u32,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Overlays {
    pub fn new() -> Self {
        let (tx, rx) = channel::<Cmd>();
        let (id_tx, id_rx) = channel::<u32>();
        let handle = std::thread::Builder::new()
            .name("penumbra-overlay".into())
            .spawn(move || run_thread(rx, id_tx))
            .expect("thread da camada");
        let thread_id = id_rx.recv().unwrap_or(0);
        Overlays { tx, thread_id, handle: Some(handle) }
    }

    fn send(&self, c: Cmd) {
        if self.tx.send(c).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_APP, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Cria/atualiza a camada do monitor. `alpha == 0` esconde a camada.
    /// Também reposiciona, então serve para mudança de resolução/DPI.
    pub fn apply(&self, m: &MonitorInfo, alpha: u8) {
        self.send(Cmd::Apply { id: m.id.clone(), x: m.x, y: m.y, w: m.width, h: m.height, alpha });
    }

    pub fn remove(&self, id: &str) {
        self.send(Cmd::Remove(id.to_string()));
    }

    pub fn remove_all(&self) {
        self.send(Cmd::RemoveAll);
    }

    /// Recoloca as camadas visíveis no topo da pilha de janelas "sempre no topo".
    pub fn reassert(&self) {
        self.send(Cmd::Reassert);
    }
}

impl Drop for Overlays {
    fn drop(&mut self) {
        self.send(Cmd::Quit);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod manual {
    use super::*;

    /// Teste manual: escurece cada monitor por ~10 s. `cargo test manual_overlay -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn manual_overlay() {
        let o = Overlays::new();
        let list = crate::monitors::list();
        for m in &list {
            println!("monitor {} {}x{} @({},{}) primary={}", m.id, m.width, m.height, m.x, m.y, m.primary);
            o.apply(m, 170);
        }
        std::thread::sleep(std::time::Duration::from_secs(10));
        o.remove_all();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn run_thread(rx: Receiver<Cmd>, id_tx: Sender<u32>) {
    unsafe {
        // Força a criação da fila de mensagens antes de publicar o id da thread.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = id_tx.send(GetCurrentThreadId());

        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: w!("PenumbraOverlay"),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&class);

        let mut windows: HashMap<String, HWND> = HashMap::new();

        'outer: while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if msg.message == WM_APP {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Cmd::Apply { id, x, y, w, h, alpha } => {
                            apply(&mut windows, hinst.into(), id, x, y, w, h, alpha)
                        }
                        Cmd::Remove(id) => {
                            if let Some(hwnd) = windows.remove(&id) {
                                let _ = DestroyWindow(hwnd);
                            }
                        }
                        Cmd::RemoveAll => {
                            for (_, hwnd) in windows.drain() {
                                let _ = DestroyWindow(hwnd);
                            }
                        }
                        Cmd::Reassert => {
                            for hwnd in windows.values() {
                                if IsWindowVisible(*hwnd).as_bool() {
                                    let _ = SetWindowPos(
                                        *hwnd,
                                        Some(HWND_TOPMOST),
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                    );
                                }
                            }
                        }
                        Cmd::Quit => break 'outer,
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for (_, hwnd) in windows.drain() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn apply(
    windows: &mut HashMap<String, HWND>,
    hinst: windows::Win32::Foundation::HINSTANCE,
    id: String,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    alpha: u8,
) {
    if alpha == 0 {
        if let Some(hwnd) = windows.get(&id) {
            let _ = ShowWindow(*hwnd, SW_HIDE);
        }
        return;
    }
    let hwnd = match windows.get(&id) {
        Some(h) => *h,
        None => {
            let created = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("PenumbraOverlay"),
                w!(""),
                WS_POPUP,
                x,
                y,
                w as i32,
                h as i32,
                None,
                None,
                Some(hinst),
                None,
            );
            match created {
                Ok(hwnd) => {
                    windows.insert(id, hwnd);
                    hwnd
                }
                Err(_) => return,
            }
        }
    };
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w as i32, h as i32, SWP_NOACTIVATE);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
}
