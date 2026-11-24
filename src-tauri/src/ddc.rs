//! Brilho real do monitor via DDC/CI (API de alto nível do dxva2).
//! As chamadas podem demorar centenas de ms: use fora da thread da UI.

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::HMONITOR;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Brightness {
    pub min: u32,
    pub current: u32,
    pub max: u32,
}

impl Brightness {
    /// Brilho atual como porcentagem 0..=100 do intervalo min..max.
    pub fn percent(&self) -> u8 {
        let span = (self.max - self.min).max(1) as f32;
        (((self.current.saturating_sub(self.min)) as f32 / span) * 100.0).round().min(100.0) as u8
    }
}

/// Converte porcentagem (0..=100, com clamp) para o valor bruto min..=max.
pub fn percent_to_raw(min: u32, max: u32, percent: u8) -> u32 {
    let p = percent.min(100) as f32 / 100.0;
    min + ((max - min) as f32 * p).round() as u32
}

/// Executa `f` com o primeiro monitor físico do HMONITOR e libera os handles sempre.
fn with_physical<T>(hmonitor: isize, f: impl FnOnce(HANDLE) -> Option<T>) -> Option<T> {
    unsafe {
        let hmon = HMONITOR(hmonitor as *mut _);
        let mut count = 0u32;
        GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count).ok()?;
        if count == 0 {
            return None;
        }
        let mut mons = vec![PHYSICAL_MONITOR::default(); count as usize];
        GetPhysicalMonitorsFromHMONITOR(hmon, &mut mons).ok()?;
        let result = f(mons[0].hPhysicalMonitor);
        for m in &mons {
            let _ = DestroyPhysicalMonitor(m.hPhysicalMonitor);
        }
        result
    }
}

/// `None` se não suportado, com erro, ou com `min == max`.
pub fn read(hmonitor: isize) -> Option<Brightness> {
    with_physical(hmonitor, |h| unsafe {
        let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
        if GetMonitorBrightness(h, &mut min, &mut cur, &mut max) == 0 || min >= max {
            return None;
        }
        Some(Brightness { min, current: cur, max })
    })
}

pub fn write_percent(hmonitor: isize, percent: u8) -> bool {
    with_physical(hmonitor, |h| unsafe {
        let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
        if GetMonitorBrightness(h, &mut min, &mut cur, &mut max) == 0 || min >= max {
            return None;
        }
        Some(SetMonitorBrightness(h, percent_to_raw(min, max, percent)) != 0)
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcentagem_para_valor_bruto() {
        assert_eq!(percent_to_raw(0, 100, 50), 50);
        assert_eq!(percent_to_raw(10, 90, 0), 10);
        assert_eq!(percent_to_raw(10, 90, 100), 90);
        assert_eq!(percent_to_raw(0, 100, 255), 100);
    }

    #[test]
    fn porcentagem_atual_do_brilho() {
        assert_eq!(Brightness { min: 10, current: 50, max: 90 }.percent(), 50);
        assert_eq!(Brightness { min: 0, current: 100, max: 100 }.percent(), 100);
    }

    /// Manual: `cargo test manual_ddc -- --ignored --nocapture`. Lê e regrava o mesmo valor (sem efeito visível).
    #[test]
    #[ignore]
    fn manual_ddc() {
        for m in crate::monitors::list() {
            let t = std::time::Instant::now();
            let r = read(m.hmonitor);
            println!("{} -> {:?} ({} ms)", m.id, r, t.elapsed().as_millis());
            if let Some(b) = r {
                let t = std::time::Instant::now();
                println!("   regravar {}% -> {} ({} ms)", b.percent(), write_percent(m.hmonitor, b.percent()), t.elapsed().as_millis());
            }
        }
    }

    /// Manual: ajusta o brilho do monitor cujo id termina em PENUMBRA_ID para PENUMBRA_PCT%.
    /// `PENUMBRA_ID=0002 PENUMBRA_PCT=100 cargo test manual_set -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn manual_set() {
        let (Ok(id), Ok(pct)) = (std::env::var("PENUMBRA_ID"), std::env::var("PENUMBRA_PCT")) else { return };
        let pct: u8 = pct.parse().unwrap();
        for m in crate::monitors::list().iter().filter(|m| m.id.ends_with(&id)) {
            println!("{} -> {}", m.id, write_percent(m.hmonitor, pct));
        }
    }

    #[test]
    fn read_de_handle_invalido_devolve_none() {
        assert_eq!(read(0), None);
    }
}
