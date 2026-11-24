pub mod core;
pub mod ddc;
pub mod levels;
pub mod monitors;
pub mod overlay;
pub mod state;

use crate::core::{Core, MonitorView};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Serializa a abertura da janela (dois cliques rápidos não criam duas janelas "main").
static OPEN_LOCK: Mutex<()> = Mutex::new(());

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Quando a janela foi fechada por perder o foco (evita reabrir no mesmo clique da bandeja).
static CLOSED_AT: AtomicU64 = AtomicU64::new(0);
/// Quando a janela foi mostrada (ignora perda de foco logo na abertura).
static SHOWN_AT: AtomicU64 = AtomicU64::new(0);
/// Se a janela atual já recebeu foco alguma vez.
static HAD_FOCUS: AtomicBool = AtomicBool::new(false);

// DDC/CI e a espera por escritas pendentes podem levar centenas de ms: rodam fora do runtime async.
#[tauri::command]
async fn list_monitors(core: State<'_, Arc<Core>>) -> Result<Vec<MonitorView>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.views()).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_level(app: AppHandle, core: State<'_, Arc<Core>>, id: String, level: i32) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.set_level(&id, level)).await.map_err(|e| e.to_string())?;
    refresh_status(&app);
    Ok(())
}

#[tauri::command]
async fn set_enabled(app: AppHandle, core: State<'_, Arc<Core>>, id: String, enabled: bool) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.set_enabled(&id, enabled)).await.map_err(|e| e.to_string())?;
    refresh_status(&app);
    Ok(())
}

/// Mesmo nível em todas as telas (fileira "Todas as telas" da interface).
#[tauri::command]
async fn set_all(app: AppHandle, core: State<'_, Arc<Core>>, level: i32) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.set_all(level)).await.map_err(|e| e.to_string())?;
    refresh_status(&app);
    Ok(())
}

const TRAY_ID: &str = "penumbra";

/// Atualiza o tooltip da bandeja com o estado de cada tela.
fn refresh_status(app: &AppHandle) {
    let core = app.state::<Arc<Core>>().inner().clone();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("Penumbra\n{}", core.summary())));
    }
}

/// Ação global (bandeja/atalho) fora da thread de eventos; depois atualiza bandeja e interface.
fn run_global(app: &AppHandle, f: impl FnOnce(&Core) + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || {
        let core = app.state::<Arc<Core>>().inner().clone();
        f(&core);
        refresh_status(&app);
        let _ = app.emit("monitors-changed", ());
    });
}

/// Largura de referência da interface (px lógicos). Janela mais larga = interface ampliada (zoom).
const BASE_WIDTH: f64 = 360.0;
const MAX_ZOOM: f64 = 2.2;
/// Zoom atual do WebView (f64 em bits).
static ZOOM_BITS: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000); // 1.0
/// Posição/tamanho atuais da janela (físicos), atualizados pelos eventos.
static CUR_GEOM: Mutex<(i32, i32, u32, u32)> = Mutex::new((0, 0, 0, 0));
/// Última posição escolhida pelo usuário nesta sessão: (x, base y, largura), em px físicos.
static LAST_GEOM: Mutex<Option<(i32, i32, u32)>> = Mutex::new(None);

fn zoom() -> f64 {
    f64::from_bits(ZOOM_BITS.load(Ordering::Relaxed))
}

/// Recalcula o zoom a partir da largura e avisa a interface (que reajusta a altura).
fn apply_zoom_for_width(w: &WebviewWindow, width_phys: u32) {
    let scale = w.scale_factor().unwrap_or(1.0);
    let z = (width_phys as f64 / scale / BASE_WIDTH).clamp(1.0, MAX_ZOOM);
    if (z - zoom()).abs() > 0.004 {
        ZOOM_BITS.store(z.to_bits(), Ordering::Relaxed);
        let _ = w.set_zoom(z);
        let _ = w.emit("zoom-changed", z);
    }
}

/// Ajusta a altura da janela ao conteúdo (`height` em px CSS, multiplicado pelo zoom), mantendo
/// a base fixa (junto à bandeja) e limitando à área útil do monitor.
#[tauri::command]
fn fit_window(app: AppHandle, height: f64) {
    let Some(w) = app.get_webview_window("main") else { return };
    let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) else { return };
    let scale = w.scale_factor().unwrap_or(1.0);
    let mut h = (height * zoom() * scale).round();
    let bottom_edge = pos.y as f64 + size.height as f64;
    let mut y = bottom_edge - h;
    if let Some(m) = w.current_monitor().ok().flatten() {
        let wa = m.work_area();
        let top = wa.position.y as f64 + 8.0;
        let bottom = wa.position.y as f64 + wa.size.height as f64 - 8.0;
        h = h.min(bottom - top);
        y = (bottom_edge - h).clamp(top, bottom - h);
    }
    let _ = w.set_size(PhysicalSize::new(size.width, h as u32));
    let _ = w.set_position(PhysicalPosition::new(pos.x, y as i32));
}

/// A interface chama quando terminou de carregar; só então a janela aparece (sem flash em branco).
#[tauri::command]
fn ui_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        SHOWN_AT.store(now_ms(), Ordering::Relaxed);
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn hide_main(app: AppHandle) {
    close_window(&app);
}

/// A janela só existe enquanto está aberta: fechada, o WebView2 não ocupa memória.
fn close_window(app: &AppHandle) {
    debug_log("close_window");
    if let Some(w) = app.get_webview_window("main") {
        std::thread::spawn(move || {
            let _ = w.destroy();
        });
    }
}

fn build_window(app: &AppHandle) -> Option<WebviewWindow> {
    // Eventos de foco logo na criação não devem fechar a janela.
    SHOWN_AT.store(now_ms() + 3000, Ordering::Relaxed);
    HAD_FOCUS.store(false, Ordering::Relaxed);
    ZOOM_BITS.store(1.0f64.to_bits(), Ordering::Relaxed);
    let w = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Penumbra")
        .inner_size(BASE_WIDTH, 560.0)
        .min_inner_size(BASE_WIDTH * 0.9, 160.0)
        .max_inner_size(BASE_WIDTH * MAX_ZOOM, 4000.0)
        .resizable(true)
        .maximizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        // Painel: fica por cima enquanto aberto (nunca abre escondido atrás de outra janela).
        .always_on_top(true)
        .visible(false)
        .build()
        .map_err(|e| eprintln!("Penumbra: falha ao criar a janela: {e:?}"))
        .ok()?;
    let handle = app.clone();
    let win = w.clone();
    w.on_window_event(move |e| match e {
        WindowEvent::Moved(p) => {
            let mut g = CUR_GEOM.lock().unwrap();
            (g.0, g.1) = (p.x, p.y);
            remember_geometry(&g);
        }
        WindowEvent::Resized(s) => {
            debug_log(&format!("Resized {}x{}", s.width, s.height));
            {
                let mut g = CUR_GEOM.lock().unwrap();
                (g.2, g.3) = (s.width, s.height);
                remember_geometry(&g);
            }
            // Getters/setters da janela fora da thread de eventos (evita travar o loop).
            let (win, width) = (win.clone(), s.width);
            std::thread::spawn(move || apply_zoom_for_width(&win, width));
        }
        WindowEvent::Focused(true) => {
            debug_log("Focused(true)");
            HAD_FOCUS.store(true, Ordering::Relaxed)
        }
        WindowEvent::Focused(false) => {
            debug_log("Focused(false)");
            // Clicar dentro da janela move o foco para o WebView (filho da janela) e o Windows
            // reporta isso como perda de foco. Só fecha se, logo depois, a janela em primeiro
            // plano for de OUTRO programa (o usuário clicou fora).
            if HAD_FOCUS.load(Ordering::Relaxed) && now_ms().saturating_sub(SHOWN_AT.load(Ordering::Relaxed)) > 400 {
                let handle = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(180));
                    if !foreground_is_ours() {
                        CLOSED_AT.store(now_ms(), Ordering::Relaxed);
                        close_window(&handle);
                    }
                });
            }
        }
        _ => {}
    });
    Some(w)
}

/// Log de diagnóstico, só com a variável de ambiente PENUMBRA_DEBUG definida.
fn debug_log(msg: &str) {
    if std::env::var_os("PENUMBRA_DEBUG").is_some() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("penumbra-debug.log"))
        {
            let _ = writeln!(f, "{} {msg}", now_ms());
        }
    }
}

/// Se a janela em primeiro plano pertence a este processo (a janela do Penumbra).
/// Sem janela em primeiro plano (acontece durante arrastos), conta como nossa.
fn foreground_is_ours() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            debug_log("foreground: nenhuma");
            return true;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if std::env::var_os("PENUMBRA_DEBUG").is_some() {
            let mut cls = [0u16; 64];
            let n = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(fg, &mut cls);
            let mut cur = windows::Win32::Foundation::POINT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cur);
            debug_log(&format!(
                "foreground pid {pid} (nosso {}) classe {} cursor {},{}",
                std::process::id(),
                String::from_utf16_lossy(&cls[..n.max(0) as usize]),
                cur.x,
                cur.y
            ));
        }
        pid == std::process::id()
    }
}

/// Guarda a geometria só quando muda com a janela já visível (movida/redimensionada pelo usuário
/// ou ajustada ao conteúdo), para reabrir no mesmo lugar e tamanho.
fn remember_geometry(g: &(i32, i32, u32, u32)) {
    if g.2 > 0 && now_ms().saturating_sub(SHOWN_AT.load(Ordering::Relaxed)) > 600 {
        *LAST_GEOM.lock().unwrap() = Some((g.0, g.1 + g.3 as i32, g.2));
    }
}

/// Abre a janela perto do ícone da bandeja. Criar janela na thread principal (dentro de um
/// callback de evento) trava no Windows, então o trabalho roda numa thread própria.
fn open_near(app: &AppHandle, anchor: Option<(f64, f64)>) {
    let app = app.clone();
    std::thread::spawn(move || open_near_blocking(&app, anchor));
}

fn open_near_blocking(app: &AppHandle, anchor: Option<(f64, f64)>) {
    let _serial = OPEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let Some(w) = build_window(app) else { return };
    // Reabre onde o usuário deixou (se esse ponto ainda está em algum monitor).
    let last = *LAST_GEOM.lock().unwrap();
    if let (Some((x, bottom, width)), Ok(size)) = (last, w.outer_size()) {
        if w.monitor_from_point(x as f64 + 20.0, bottom as f64 - 20.0).ok().flatten().is_some() {
            let _ = w.set_size(PhysicalSize::new(width, size.height));
            let _ = w.set_position(PhysicalPosition::new(x, bottom - size.height as i32));
            return;
        }
    }
    if let Ok(size) = w.outer_size() {
        let (ax, ay) = anchor.unwrap_or_else(|| {
            let m = monitors::list().into_iter().find(|m| m.primary);
            m.map(|m| (m.x as f64 + m.width as f64 - 240.0, m.y as f64 + m.height as f64 - 48.0)).unwrap_or((800.0, 800.0))
        });
        let monitor = w.monitor_from_point(ax, ay).ok().flatten().or_else(|| w.primary_monitor().ok().flatten());
        let (mut x, mut y) = (ax - size.width as f64 / 2.0, ay - size.height as f64 - 12.0);
        if let Some(m) = monitor {
            let wa = m.work_area();
            let (l, t) = (wa.position.x as f64, wa.position.y as f64);
            let (r, b) = (l + wa.size.width as f64, t + wa.size.height as f64);
            x = x.clamp(l + 8.0, (r - size.width as f64 - 8.0).max(l));
            y = y.clamp(t + 8.0, (b - size.height as f64 - 8.0).max(t));
        }
        let _ = w.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
}

fn toggle(app: &AppHandle, anchor: Option<(f64, f64)>) {
    if app.get_webview_window("main").is_some() {
        close_window(app);
    } else if now_ms().saturating_sub(CLOSED_AT.load(Ordering::Relaxed)) > 400 {
        open_near(app, anchor);
    }
}

pub fn run() {
    // Coordenadas físicas corretas em monitores com escalas diferentes, já na primeira listagem.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let core = Core::new(core::config_path());
    let core_setup = core.clone();

    // Atalhos globais: escurecer/clarear todas em 10% e liga/desliga tudo.
    let ctrl_alt = Some(Modifiers::CONTROL | Modifiers::ALT);
    let sc_dim = Shortcut::new(ctrl_alt, Code::PageDown);
    let sc_bright = Shortcut::new(ctrl_alt, Code::PageUp);
    let sc_toggle = Shortcut::new(ctrl_alt, Code::End);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_near(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, sc, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    if sc == &sc_dim {
                        run_global(app, |c| c.step_all(10));
                    } else if sc == &sc_bright {
                        run_global(app, |c| c.step_all(-10));
                    } else if sc == &sc_toggle {
                        run_global(app, |c| c.toggle_all());
                    }
                })
                .build(),
        )
        .manage(core.clone())
        .invoke_handler(tauri::generate_handler![
            list_monitors, set_level, set_enabled, set_all, hide_main, ui_ready, fit_window
        ])
        .setup(move |app| {
            // Atalhos (se outro programa já usa a combinação, o registro falha e seguimos sem ela).
            for sc in [sc_dim, sc_bright, sc_toggle] {
                if let Err(e) = app.global_shortcut().register(sc) {
                    eprintln!("Penumbra: atalho indisponível: {e}");
                }
            }

            // Bandeja.
            let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
            let open = MenuItem::with_id(app, "open", "Abrir o Penumbra", true, None::<&str>)?;
            let dim25 = MenuItem::with_id(app, "dim25", "Escurecer todas · 25%", true, None::<&str>)?;
            let dim50 = MenuItem::with_id(app, "dim50", "Escurecer todas · 50%", true, None::<&str>)?;
            let dim75 = MenuItem::with_id(app, "dim75", "Escurecer todas · 75%", true, None::<&str>)?;
            let restore = MenuItem::with_id(app, "restore", "Restaurar todas", true, None::<&str>)?;
            let autostart = CheckMenuItem::with_id(app, "autostart", "Iniciar com o Windows", true, autostart_on, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let sep = || PredefinedMenuItem::separator(app);
            let menu = Menu::with_items(
                app,
                &[&open, &sep()?, &dim25, &dim50, &dim75, &restore, &sep()?, &autostart, &sep()?, &quit],
            )?;
            let autostart_item = autostart.clone();
            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip("Penumbra")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, ev| match ev.id.as_ref() {
                    "open" => open_near(app, None),
                    "dim25" => run_global(app, |c| c.set_all(25)),
                    "dim50" => run_global(app, |c| c.set_all(50)),
                    "dim75" => run_global(app, |c| c.set_all(75)),
                    "restore" => run_global(app, |c| c.set_all(0)),
                    "autostart" => {
                        let al = app.autolaunch();
                        let on = al.is_enabled().unwrap_or(false);
                        let _ = if on { al.disable() } else { al.enable() };
                        let _ = autostart_item.set_checked(al.is_enabled().unwrap_or(!on));
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = ev {
                        let pos: PhysicalPosition<f64> = rect.position.to_physical(1.0);
                        let size: tauri::PhysicalSize<f64> = rect.size.to_physical(1.0);
                        toggle(tray.app_handle(), Some((pos.x + size.width / 2.0, pos.y)));
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Aplica a configuração salva e vigia plugar/desplugar monitores.
            let c = core_setup.clone();
            let handle = app.handle().clone();
            std::thread::Builder::new().name("penumbra-watch".into()).spawn(move || {
                c.apply_all();
                refresh_status(&handle);
                let _ = handle.emit("monitors-changed", ());
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    if c.refresh_monitors() {
                        c.apply_all();
                        refresh_status(&handle);
                        let _ = handle.emit("monitors-changed", ());
                    }
                    c.reassert_overlays();
                }
            })?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Penumbra");

    let core_exit = core;
    app.run(move |_app, event| match event {
        // Fechar a janela não encerra o app: ele continua na bandeja.
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => core_exit.restore_all(),
        _ => {}
    });
}
