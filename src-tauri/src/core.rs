//! Núcleo: junta monitores, camada, DDC/CI e configuração.

use crate::levels::{self, Split};
use crate::monitors::{self, MonitorInfo};
use crate::overlay::Overlays;
use crate::state::{self, Config, MonitorConfig};
use crate::ddc;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(150);

/// O DDC só entra se o monitor suporta e, quando escurecendo, o brilho original é conhecido.
pub fn ddc_usable(supported: bool, dimming: bool, original: Option<u8>) -> bool {
    supported && (!dimming || original.is_some())
}

/// Monitor desligado equivale a nível 0.
pub fn plan(level: u8, enabled: bool, original: u8, ddc_supported: bool) -> Split {
    levels::split(if enabled { level } else { 0 }, original, ddc_supported)
}

#[derive(Serialize, Clone, Debug)]
pub struct MonitorView {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub level: u8,
    pub enabled: bool,
    pub ddc: bool,
}

struct Inner {
    config: Config,
    config_path: PathBuf,
    monitors: Vec<MonitorInfo>,
    ddc_ok: HashMap<String, bool>,
}

enum Msg {
    Ddc { id: String, hmonitor: isize, percent: u8, clear_original: bool },
    Save,
    Flush(Sender<()>),
    Quit,
}

pub struct Core {
    inner: Arc<Mutex<Inner>>,
    overlays: Overlays,
    worker: Sender<Msg>,
    apply_lock: Mutex<()>,
    /// Níveis guardados pelo "liga/desliga tudo" (Ctrl+Alt+End).
    toggled_off: Mutex<Vec<(String, u8)>>,
}

impl Core {
    pub fn new(config_path: PathBuf) -> Arc<Core> {
        let inner = Arc::new(Mutex::new(Inner {
            config: Config::load(&config_path),
            config_path,
            monitors: monitors::list(),
            ddc_ok: HashMap::new(),
        }));
        let (tx, rx) = channel::<Msg>();
        let winner = inner.clone();
        std::thread::Builder::new()
            .name("penumbra-worker".into())
            .spawn(move || worker_loop(rx, winner))
            .expect("thread do worker");
        Arc::new(Core {
            inner,
            overlays: Overlays::new(),
            worker: tx,
            apply_lock: Mutex::new(()),
            toggled_off: Mutex::new(Vec::new()),
        })
    }

    /// Reenumera monitores; devolve true se o conjunto (ou o handle de algum) mudou.
    pub fn refresh_monitors(&self) -> bool {
        let fresh = monitors::list();
        let mut g = self.inner.lock().unwrap();
        let same_handles = g.monitors.len() == fresh.len()
            && g.monitors.iter().all(|o| fresh.iter().any(|n| n.id == o.id && n.hmonitor == o.hmonitor));
        if monitors::same_set(&g.monitors, &fresh) && same_handles {
            return false;
        }
        // Monitor que falhou na sondagem pode ter sido só uma falha passageira: tenta de novo.
        g.ddc_ok.retain(|_, ok| *ok);
        let gone: Vec<String> = g
            .monitors
            .iter()
            .filter(|o| !fresh.iter().any(|n| n.id == o.id))
            .map(|o| o.id.clone())
            .collect();
        g.monitors = fresh;
        drop(g);
        for id in gone {
            self.overlays.remove(&id);
        }
        true
    }

    /// Testa o suporte a DDC/CI dos monitores ainda não testados (lento; fora do lock).
    fn probe_all(&self) {
        let todo: Vec<(String, isize)> = {
            let g = self.inner.lock().unwrap();
            g.monitors
                .iter()
                .filter(|m| !g.ddc_ok.contains_key(&m.id))
                .map(|m| (m.id.clone(), m.hmonitor))
                .collect()
        };
        for (id, h) in todo {
            let ok = ddc::read(h).is_some();
            self.inner.lock().unwrap().ddc_ok.insert(id, ok);
        }
    }

    /// O brilho original só vale enquanto o monitor está escurecido: guarda ao começar a escurecer.
    fn ensure_original(&self, id: &str, hmonitor: isize) {
        let (need, supported) = {
            let g = self.inner.lock().unwrap();
            (
                g.config.get(id).original_brightness.is_none(),
                g.ddc_ok.get(id).copied().unwrap_or(false),
            )
        };
        if !need || !supported {
            return;
        }
        // Escritas DDC pendentes (ex.: restauração a 0 logo antes) precisam chegar ao monitor antes da leitura.
        let (tx, rx) = channel();
        if self.worker.send(Msg::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
        if let Some(b) = ddc::read(hmonitor) {
            let mut g = self.inner.lock().unwrap();
            let mut c = g.config.get(id);
            c.original_brightness = Some(b.percent());
            g.config.set(id, c);
        }
    }

    /// Aplica a configuração atual de um monitor. Serializado: comandos concorrentes
    /// (arrasto rápido do slider) executam um de cada vez e o último vence com a config mais nova.
    fn apply(&self, id: &str) {
        let _serial = self.apply_lock.lock().unwrap();
        let (m, supported) = {
            let g = self.inner.lock().unwrap();
            let Some(m) = g.monitors.iter().find(|m| m.id == id).cloned() else { return };
            (m, g.ddc_ok.get(id).copied().unwrap_or(false))
        };
        let cfg = self.inner.lock().unwrap().config.get(id);
        let dimming = cfg.enabled && cfg.level > 0;
        if supported && dimming {
            self.ensure_original(id, m.hmonitor);
        }
        let cfg = self.inner.lock().unwrap().config.get(id);
        // Se a leitura do brilho original falhou, não mexe no DDC: usa só a camada.
        let use_ddc = ddc_usable(supported, dimming, cfg.original_brightness);
        let split = plan(cfg.level, cfg.enabled, cfg.original_brightness.unwrap_or(100), use_ddc);
        self.overlays.apply(&m, split.overlay_alpha);
        // Sem escurecer e sem original guardado, o monitor já está como o usuário deixou: não escreve.
        let must_write = dimming || cfg.original_brightness.is_some();
        if let (true, true, Some(p)) = (use_ddc, must_write, split.ddc_percent) {
            // Ao restaurar, o original só é descartado depois que a escrita der certo (no worker).
            let _ = self.worker.send(Msg::Ddc {
                id: m.id.clone(),
                hmonitor: m.hmonitor,
                percent: p,
                clear_original: !dimming,
            });
        }
    }

    /// Aplica a configuração salva a todos os monitores presentes (início e troca de monitores).
    pub fn apply_all(&self) {
        self.probe_all();
        let present: Vec<MonitorInfo> = self.inner.lock().unwrap().monitors.clone();
        for m in &present {
            self.apply(&m.id);
        }
        let _ = self.worker.send(Msg::Save);
    }

    pub fn views(&self) -> Vec<MonitorView> {
        self.probe_all();
        let g = self.inner.lock().unwrap();
        g.monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let c = g.config.get(&m.id);
                MonitorView {
                    id: m.id.clone(),
                    index: i + 1,
                    name: m.name.clone(),
                    primary: m.primary,
                    x: m.x,
                    y: m.y,
                    width: m.width,
                    height: m.height,
                    level: c.level,
                    enabled: c.enabled,
                    ddc: g.ddc_ok.get(&m.id).copied().unwrap_or(false),
                }
            })
            .collect()
    }

    pub fn set_level(&self, id: &str, level: i32) {
        {
            let mut g = self.inner.lock().unwrap();
            let mut c = g.config.get(id);
            c.level = levels::clamp_level(level);
            // Mexer no slider liga o monitor.
            c.enabled = c.level > 0 || c.enabled;
            g.config.set(id, c);
        }
        self.apply(id);
        let _ = self.worker.send(Msg::Save);
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) {
        {
            let mut g = self.inner.lock().unwrap();
            let mut c: MonitorConfig = g.config.get(id);
            c.enabled = enabled;
            g.config.set(id, c);
        }
        self.apply(id);
        let _ = self.worker.send(Msg::Save);
    }

    fn present_levels(&self) -> Vec<(String, u8, bool)> {
        let g = self.inner.lock().unwrap();
        g.monitors
            .iter()
            .map(|m| {
                let c = g.config.get(&m.id);
                (m.id.clone(), c.level, c.enabled)
            })
            .collect()
    }

    /// Mesmo nível em todas as telas presentes.
    pub fn set_all(&self, level: i32) {
        for (id, _, _) in self.present_levels() {
            self.set_level(&id, level);
        }
    }

    /// Escurece (+) ou clareia (−) todas as telas pelo mesmo passo.
    pub fn step_all(&self, delta: i32) {
        for (id, level, enabled) in self.present_levels() {
            let base = if enabled { level as i32 } else { 0 };
            self.set_level(&id, base + delta);
        }
    }

    /// Liga/desliga tudo: se algo está escurecido, guarda os níveis e zera; senão, devolve os
    /// níveis guardados (ou 50% se não houver).
    pub fn toggle_all(&self) {
        let now = self.present_levels();
        let mut saved = self.toggled_off.lock().unwrap();
        if now.iter().any(|(_, l, e)| *e && *l > 0) {
            *saved = now.iter().filter(|(_, l, e)| *e && *l > 0).map(|(id, l, _)| (id.clone(), *l)).collect();
            drop(saved);
            self.set_all(0);
        } else {
            let restore: Vec<(String, u8)> = if saved.is_empty() {
                now.iter().map(|(id, _, _)| (id.clone(), 50)).collect()
            } else {
                saved.drain(..).collect()
            };
            drop(saved);
            for (id, l) in restore {
                self.set_level(&id, l as i32);
            }
        }
    }

    /// Resumo para o tooltip da bandeja, ex.: "Tela 1: 35% · Tela 2: normal".
    pub fn summary(&self) -> String {
        self.present_levels()
            .iter()
            .enumerate()
            .map(|(i, (_, l, e))| {
                if *e && *l > 0 { format!("Tela {}: {}%", i + 1, l) } else { format!("Tela {}: normal", i + 1) }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Reafirma "sempre no topo" das camadas (janelas topmost criadas depois não ficam por cima).
    pub fn reassert_overlays(&self) {
        self.overlays.reassert();
    }

    /// Remove as camadas e devolve o brilho DDC original. Chamado ao sair.
    pub fn restore_all(&self) {
        self.overlays.remove_all();
        let (tx, rx) = channel();
        let _ = self.worker.send(Msg::Flush(tx));
        let _ = rx.recv_timeout(Duration::from_secs(3));
        let todo: Vec<(String, isize, u8)> = {
            let g = self.inner.lock().unwrap();
            g.monitors
                .iter()
                .filter(|m| g.ddc_ok.get(&m.id).copied().unwrap_or(false))
                .filter_map(|m| g.config.get(&m.id).original_brightness.map(|o| (m.id.clone(), m.hmonitor, o)))
                .collect()
        };
        let restored: Vec<String> = todo
            .into_iter()
            .filter(|(_, h, o)| ddc::write_percent(*h, *o))
            .map(|(id, _, _)| id)
            .collect();
        // Só descarta o original dos monitores cuja restauração deu certo.
        let mut g = self.inner.lock().unwrap();
        for id in restored {
            let mut c = g.config.get(&id);
            c.original_brightness = None;
            g.config.set(&id, c);
        }
        let _ = g.config.save(&g.config_path);
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.worker.send(Msg::Quit);
    }
}

fn worker_loop(rx: std::sync::mpsc::Receiver<Msg>, inner: Arc<Mutex<Inner>>) {
    // id -> (hmonitor, percentual, descartar o original se a escrita der certo)
    let mut pending: HashMap<String, (isize, u8, bool)> = HashMap::new();
    let mut dirty = false;
    let mut deadline: Option<Instant> = None;

    let flush = |pending: &mut HashMap<String, (isize, u8, bool)>, dirty: &mut bool| {
        for (id, (h, p, clear)) in pending.drain() {
            let ok = ddc::write_percent(h, p);
            if ok && clear {
                // Restaurado de fato: o original deixa de valer (ajustes manuais no monitor passam a valer).
                let mut g = inner.lock().unwrap();
                let mut c = g.config.get(&id);
                if c.original_brightness.is_some() {
                    c.original_brightness = None;
                    g.config.set(&id, c);
                    *dirty = true;
                }
            }
        }
        if *dirty {
            let g = inner.lock().unwrap();
            let _ = g.config.save(&g.config_path);
            *dirty = false;
        }
    };

    loop {
        let msg = match deadline {
            None => rx.recv().ok(),
            Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => {
                    flush(&mut pending, &mut dirty);
                    deadline = None;
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => None,
            },
        };
        match msg {
            None | Some(Msg::Quit) => {
                flush(&mut pending, &mut dirty);
                break;
            }
            Some(Msg::Ddc { id, hmonitor, percent, clear_original }) => {
                pending.insert(id, (hmonitor, percent, clear_original));
                deadline.get_or_insert_with(|| Instant::now() + DEBOUNCE);
            }
            Some(Msg::Save) => {
                dirty = true;
                deadline.get_or_insert_with(|| Instant::now() + DEBOUNCE);
            }
            Some(Msg::Flush(ack)) => {
                flush(&mut pending, &mut dirty);
                deadline = None;
                let _ = ack.send(());
            }
        }
    }
}

pub fn config_path() -> PathBuf {
    state::default_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual: `cargo test manual_core -- --ignored --nocapture`. Escurece tudo a 70% por ~8 s e restaura.
    #[test]
    #[ignore]
    fn manual_core() {
        let path = std::env::temp_dir().join("penumbra-manual-core.json");
        let _ = std::fs::remove_file(&path);
        let c = Core::new(path);
        let views = c.views();
        let show = |tag: &str| {
            for m in monitors::list() {
                println!("{tag}: {} -> {:?}", &m.id[m.id.len() - 4..], ddc::read(m.hmonitor).map(|b| b.percent()));
            }
        };
        show("antes");
        for v in &views {
            c.set_level(&v.id, 70);
        }
        std::thread::sleep(Duration::from_secs(4));
        show("escurecido");
        // Arrasto rápido e concorrente: o original não pode ser sobrescrito com valor já escurecido.
        let c2 = c.clone();
        let ids: Vec<String> = views.iter().map(|v| v.id.clone()).collect();
        let hs: Vec<_> = (0..4)
            .map(|t| {
                let (c, ids) = (c2.clone(), ids.clone());
                std::thread::spawn(move || {
                    for i in 0..15 {
                        for id in &ids {
                            c.set_level(id, if (i + t) % 2 == 0 { 0 } else { 40 });
                        }
                    }
                })
            })
            .collect();
        for h in hs {
            h.join().unwrap();
        }
        for id in &ids {
            c.set_level(id, 70);
        }
        std::thread::sleep(Duration::from_secs(1));
        show("apos arrasto rapido (70)");
        c.restore_all();
        show("restaurado");
    }

    #[test]
    fn ddc_so_entra_se_suportado_e_com_original_conhecido_ao_escurecer() {
        assert!(!ddc_usable(false, true, Some(80)));
        assert!(!ddc_usable(true, true, None)); // leitura do original falhou: só camada
        assert!(ddc_usable(true, true, Some(80)));
        assert!(ddc_usable(true, false, Some(80))); // restaurando
        assert!(ddc_usable(true, false, None)); // sem escurecer: permitido, mas apply não escreve sem original
    }

    #[test]
    fn desligado_equivale_a_nivel_zero() {
        let s = plan(80, false, 70, true);
        assert_eq!(s.overlay_alpha, 0);
        assert_eq!(s.ddc_percent, Some(70));
    }

    #[test]
    fn ligado_usa_o_nivel() {
        assert_eq!(plan(90, true, 100, false).overlay_alpha, 230);
    }
}
