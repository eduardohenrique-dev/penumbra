//! Enumeração de monitores via Win32.

use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
    MONITORINFOEXW,
};

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct MonitorInfo {
    /// Id estável (DeviceID do monitor), não o índice.
    pub id: String,
    pub name: String,
    /// Posição e tamanho em pixels físicos.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub hmonitor: isize,
}

/// Compara dois conjuntos de monitores por id e geometria, ignorando a ordem e o handle.
pub fn same_set(a: &[MonitorInfo], b: &[MonitorInfo]) -> bool {
    fn key(m: &MonitorInfo) -> (String, i32, i32, u32, u32, bool) {
        (m.id.clone(), m.x, m.y, m.width, m.height, m.primary)
    }
    let mut ka: Vec<_> = a.iter().map(key).collect();
    let mut kb: Vec<_> = b.iter().map(key).collect();
    ka.sort();
    kb.sort();
    ka == kb
}

fn wstr(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> windows::core::BOOL {
    let out = &mut *(data.0 as *mut Vec<MonitorInfo>);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !GetMonitorInfoW(hmon, &mut mi as *mut _ as *mut _).as_bool() {
        return true.into();
    }
    let r = mi.monitorInfo.rcMonitor;
    let gdi_name = wstr(&mi.szDevice);

    let mut dd = DISPLAY_DEVICEW::default();
    dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    let (id, name) = if EnumDisplayDevicesW(PCWSTR(mi.szDevice.as_ptr()), 0, &mut dd, 0).as_bool() {
        (wstr(&dd.DeviceID), wstr(&dd.DeviceString))
    } else {
        (gdi_name.clone(), String::new())
    };

    out.push(MonitorInfo {
        id: if id.is_empty() { gdi_name } else { id },
        name,
        x: r.left,
        y: r.top,
        width: (r.right - r.left).max(0) as u32,
        height: (r.bottom - r.top).max(0) as u32,
        primary: mi.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        hmonitor: hmon.0 as isize,
    });
    true.into()
}

pub fn list() -> Vec<MonitorInfo> {
    let mut v: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|m| (!m.primary, m.x, m.y));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, x: i32, w: u32) -> MonitorInfo {
        MonitorInfo { id: id.into(), name: String::new(), x, y: 0, width: w, height: 1080, primary: x == 0, hmonitor: 0 }
    }

    #[test]
    fn mesma_lista_em_outra_ordem_e_igual() {
        assert!(same_set(&[m("A", 0, 1920), m("B", 1920, 1920)], &[m("B", 1920, 1920), m("A", 0, 1920)]));
    }

    #[test]
    fn geometria_diferente_nao_e_igual() {
        assert!(!same_set(&[m("A", 0, 1920)], &[m("A", 0, 2560)]));
    }

    #[test]
    fn monitor_a_mais_nao_e_igual() {
        assert!(!same_set(&[m("A", 0, 1920)], &[m("A", 0, 1920), m("B", 1920, 1920)]));
    }

    #[test]
    fn list_nao_entra_em_panico_e_ids_sao_unicos() {
        let l = list();
        let mut ids: Vec<_> = l.iter().map(|m| m.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), l.len());
    }
}
