//! Configuração persistente por monitor (JSON em %APPDATA%\Penumbra\config.json).

use crate::levels::clamp_level;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct MonitorConfig {
    pub level: u8,
    pub enabled: bool,
    /// Brilho DDC/CI original, guardado na primeira escrita para restaurar ao sair.
    pub original_brightness: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub monitors: HashMap<String, MonitorConfig>,
}

impl Config {
    /// Lê o arquivo; qualquer problema (ausente, vazio, corrompido) devolve o padrão.
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }

    pub fn get(&self, id: &str) -> MonitorConfig {
        self.monitors.get(id).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, id: &str, mut c: MonitorConfig) {
        c.level = clamp_level(c.level as i32);
        self.monitors.insert(id.to_string(), c);
    }
}

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Penumbra").join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("penumbra-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("config.json")
    }

    #[test]
    fn load_de_arquivo_inexistente_devolve_padrao() {
        assert_eq!(Config::load(&tmp("ausente")), Config::default());
    }

    #[test]
    fn load_de_arquivo_vazio_ou_corrompido_devolve_padrao() {
        let p = tmp("corrompido");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        assert_eq!(Config::load(&p), Config::default());
        std::fs::write(&p, "{ isto nao e json").unwrap();
        assert_eq!(Config::load(&p), Config::default());
    }

    #[test]
    fn salvar_e_carregar_preserva_dois_monitores() {
        let p = tmp("roundtrip");
        let mut c = Config::default();
        c.set("A", MonitorConfig { level: 40, enabled: true, original_brightness: Some(80) });
        c.set("B", MonitorConfig { level: 10, enabled: false, original_brightness: None });
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
    }

    #[test]
    fn set_limita_o_nivel_a_90() {
        let mut c = Config::default();
        c.set("A", MonitorConfig { level: 200, enabled: true, original_brightness: None });
        assert_eq!(c.get("A").level, 90);
    }

    #[test]
    fn get_de_id_desconhecido_devolve_padrao() {
        let c = Config::default();
        assert_eq!(c.get("nada"), MonitorConfig { level: 0, enabled: false, original_brightness: None });
    }

    #[test]
    fn monitor_ausente_continua_no_arquivo_apos_salvar() {
        let p = tmp("ausente-preservado");
        let mut c = Config::default();
        c.set("DESCONECTADO", MonitorConfig { level: 55, enabled: true, original_brightness: None });
        c.save(&p).unwrap();
        let mut again = Config::load(&p);
        again.set("OUTRO", MonitorConfig::default());
        again.save(&p).unwrap();
        assert_eq!(Config::load(&p).get("DESCONECTADO").level, 55);
    }
}
