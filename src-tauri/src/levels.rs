//! Matemática pura de níveis de escurecimento.
//!
//! Nível 0..=90 = percentual de luz removido. Luz restante T = 1 - nível/100,
//! então o nível 90 sempre deixa 10% de luz, com ou sem DDC/CI.
//! Com DDC/CI o brilho real cai até T = 0,5 (nível 50); o que passar disso
//! vira camada escura sobre a tela.

pub const MAX_LEVEL: u8 = 90;
const DDC_MAX_LEVEL: u8 = 50;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Split {
    /// Brilho real do monitor (0..=100), se o DDC/CI for usado.
    pub ddc_percent: Option<u8>,
    /// Opacidade da camada preta (0..=255).
    pub overlay_alpha: u8,
}

pub fn clamp_level(n: i32) -> u8 {
    n.clamp(0, MAX_LEVEL as i32) as u8
}

pub fn split(level: u8, original_brightness: u8, ddc_supported: bool) -> Split {
    let level = level.min(MAX_LEVEL);
    let original = original_brightness.min(100);
    let remaining = 1.0 - level as f32 / 100.0;

    if !ddc_supported {
        return Split {
            ddc_percent: None,
            overlay_alpha: (level as f32 / 100.0 * 255.0).round() as u8,
        };
    }

    if level <= DDC_MAX_LEVEL {
        Split {
            ddc_percent: Some((original as f32 * remaining).round() as u8),
            overlay_alpha: 0,
        }
    } else {
        Split {
            ddc_percent: Some((original as f32 * 0.5).round() as u8),
            overlay_alpha: ((1.0 - remaining / 0.5) * 255.0).round() as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_limita_a_0_e_90() {
        assert_eq!(clamp_level(-5), 0);
        assert_eq!(clamp_level(200), 90);
        assert_eq!(clamp_level(37), 37);
    }

    #[test]
    fn nivel_zero_com_ddc_restaura_o_brilho_original() {
        let s = split(0, 70, true);
        assert_eq!(s.ddc_percent, Some(70));
        assert_eq!(s.overlay_alpha, 0);
    }

    #[test]
    fn nivel_zero_sem_ddc_nao_tem_camada() {
        assert_eq!(split(0, 70, false), Split { ddc_percent: None, overlay_alpha: 0 });
    }

    #[test]
    fn ddc_cobre_ate_o_nivel_50_sem_camada() {
        assert_eq!(split(50, 100, true), Split { ddc_percent: Some(50), overlay_alpha: 0 });
        assert_eq!(split(30, 80, true), Split { ddc_percent: Some(56), overlay_alpha: 0 });
    }

    #[test]
    fn acima_de_50_o_brilho_trava_na_metade_e_a_camada_completa() {
        assert_eq!(split(60, 100, true), Split { ddc_percent: Some(50), overlay_alpha: 51 });
        assert_eq!(split(90, 100, true), Split { ddc_percent: Some(50), overlay_alpha: 204 });
    }

    #[test]
    fn sem_ddc_a_camada_vai_ate_230_nunca_255() {
        assert_eq!(split(60, 100, false), Split { ddc_percent: None, overlay_alpha: 153 });
        assert_eq!(split(90, 100, false).overlay_alpha, 230);
        assert_eq!(split(255, 100, false).overlay_alpha, 230);
    }

    #[test]
    fn luz_restante_igual_com_e_sem_ddc_no_nivel_maximo() {
        // 10% de luz nos dois casos: 0,5 * (1 - 204/255) ≈ 0,1 ; 1 * (1 - 230/255) ≈ 0,0980
        let com = 0.5 * (1.0 - split(90, 100, true).overlay_alpha as f32 / 255.0);
        let sem = 1.0 - split(90, 100, false).overlay_alpha as f32 / 255.0;
        assert!((com - sem).abs() < 0.01);
    }
}
