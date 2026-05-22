//! [M12.7] Hardware probe — определение Apple Silicon / RAM / Metal +
//! рекомендация preset'а.
//!
//! См. PRD §M12.7. Запускается один раз при первом открытии Settings →
//! «Движок», результат кэшируется в `settings.hw_report` JSON.
//!
//! # Recommendation rules (PRD §M12.7.2)
//!
//! | Hardware                                   | Рекомендация      |
//! |--------------------------------------------|-------------------|
//! | Apple Silicon M-series + 8 GB              | Light             |
//! | Apple Silicon M-series + 16 GB             | Balanced          |
//! | Apple Silicon Pro/Max/Ultra + 16+ GB       | Balanced (Quality avail) |
//! | Intel Mac + 16 GB                          | Light + warning (no Metal) |
//! | Intel Mac + 8 GB                           | Light + warning «~30min на час аудио» |
//! | Linux / Windows                            | `None` (R9 — local не предлагается) |

use serde::{Deserialize, Serialize};

use super::preset::LocalEnginePreset;

/// Snapshot железа. Совместим с contract'ом
/// `packages/contracts/src/local-engine.ts::HwReport`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct HwReport {
    pub os: HwOs,
    pub arch: HwArch,
    pub cpu_model: String,
    pub ram_gb: u64,
    pub metal_supported: bool,
    /// `None` на не-macOS платформах (R9) — UI скрывает Local engine option.
    pub recommendation: Option<LocalEnginePreset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HwOs {
    Macos,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HwArch {
    #[serde(rename = "arm64")]
    Arm64,
    /// Apple/Intel x86_64 — wire-format `x86_64` (matches contract TS literal).
    #[serde(rename = "x86_64")]
    X8664,
}

/// Pure-функция выбора preset'а от параметров — отдельно от platform-detect
/// чтобы её можно было тестировать на всех 5 сценариях из PRD M12.7.2.
///
/// `ram_gb == 0` → `None`: sysctl не отдал hw.memsize (sandbox / контейнер /
/// необычный run). UI обязан показать «Не удалось определить железо,
/// выберите preset вручную» вместо тихого Light fallback'а — иначе юзер
/// рискует получить «работает но медленно» без понимания почему.
pub fn recommend_preset(
    os: HwOs,
    arch: HwArch,
    ram_gb: u64,
    metal_supported: bool,
    is_apple_silicon_pro_max: bool,
) -> Option<LocalEnginePreset> {
    // R9 — не-macOS: local не предлагается.
    if os != HwOs::Macos {
        return None;
    }
    if ram_gb == 0 {
        return None;
    }
    let is_apple_silicon = arch == HwArch::Arm64 && metal_supported;
    match (is_apple_silicon, is_apple_silicon_pro_max, ram_gb) {
        // Apple Silicon Pro/Max/Ultra с 16+ GB → Balanced (Quality доступен).
        (true, true, r) if r >= 16 => Some(LocalEnginePreset::Balanced),
        // Apple Silicon обычный 16+ GB → Balanced.
        (true, false, r) if r >= 16 => Some(LocalEnginePreset::Balanced),
        // Apple Silicon 8 GB → Light.
        (true, _, r) if r >= 8 => Some(LocalEnginePreset::Light),
        // Intel Mac (любой RAM) → Light + warning в UI (без Metal медленно).
        (false, _, _) => Some(LocalEnginePreset::Light),
        // Apple Silicon < 8 GB (теоретически не существует, но guard'имся) → Light.
        _ => Some(LocalEnginePreset::Light),
    }
}

/// Узнать железо. На macOS — реальный probe через `sysctl`. На других —
/// (R9) безусловно `os = linux/windows` + `recommendation = None`.
#[cfg(target_os = "macos")]
pub fn probe_hardware() -> HwReport {
    use std::process::Command;

    fn sysctl(key: &str) -> Option<String> {
        Command::new("sysctl")
            .arg("-n")
            .arg(key)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }

    let cpu_model = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "unknown".to_string());
    let ram_bytes: u64 = sysctl("hw.memsize")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let ram_gb = (ram_bytes as f64 / 1024.0_f64.powi(3)).round() as u64;
    // `hw.optional.arm64` == 1 на Apple Silicon.
    let is_arm64 =
        sysctl("hw.optional.arm64").as_deref() == Some("1") || std::env::consts::ARCH == "aarch64";
    let arch = if is_arm64 {
        HwArch::Arm64
    } else {
        HwArch::X8664
    };
    // Metal поддерживается на всех Mac с Apple Silicon (arm64) и на Intel
    // GPU начиная с macOS 10.11 (но варианты бывают). Простая эвристика:
    // arm64 → Metal есть; иначе — Metal-feasible но без гарантии.
    let metal_supported = is_arm64;
    // Apple Silicon Pro/Max/Ultra detect по CPU brand: «Apple M{N} Pro»,
    // «Apple M{N} Max», «Apple M{N} Ultra».
    let is_pro_max =
        cpu_model.contains("Pro") || cpu_model.contains("Max") || cpu_model.contains("Ultra");
    let recommendation = recommend_preset(HwOs::Macos, arch, ram_gb, metal_supported, is_pro_max);
    HwReport {
        os: HwOs::Macos,
        arch,
        cpu_model,
        ram_gb,
        metal_supported,
        recommendation,
    }
}

/// Non-macOS stub. R9 — local engine недоступен.
#[cfg(not(target_os = "macos"))]
pub fn probe_hardware() -> HwReport {
    let os = if cfg!(target_os = "windows") {
        HwOs::Windows
    } else {
        HwOs::Linux
    };
    HwReport {
        os,
        arch: if std::env::consts::ARCH == "aarch64" {
            HwArch::Arm64
        } else {
            HwArch::X8664
        },
        cpu_model: "unknown".to_string(),
        ram_gb: 0,
        metal_supported: false,
        recommendation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_and_windows_get_no_recommendation_r9() {
        assert_eq!(
            recommend_preset(HwOs::Linux, HwArch::X8664, 32, false, false),
            None
        );
        assert_eq!(
            recommend_preset(HwOs::Windows, HwArch::Arm64, 16, false, false),
            None
        );
    }

    #[test]
    fn apple_silicon_8gb_recommends_light() {
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::Arm64, 8, true, false),
            Some(LocalEnginePreset::Light)
        );
    }

    #[test]
    fn apple_silicon_16gb_recommends_balanced() {
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::Arm64, 16, true, false),
            Some(LocalEnginePreset::Balanced)
        );
    }

    #[test]
    fn apple_silicon_pro_recommends_balanced() {
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::Arm64, 32, true, true),
            Some(LocalEnginePreset::Balanced)
        );
    }

    #[test]
    fn intel_mac_recommends_light_regardless_of_ram() {
        // Intel = no Metal arm64 → Light + warning (warning эмит'ит UI).
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::X8664, 16, false, false),
            Some(LocalEnginePreset::Light)
        );
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::X8664, 8, false, false),
            Some(LocalEnginePreset::Light)
        );
    }

    #[test]
    fn arch_wire_format_matches_contract() {
        // contract TS: type Arch = 'arm64' | 'x86_64'. Регрессия: рефактор
        // enum rename легко ломает wire-формат и сериализация уйдёт в
        // несовместимое `x8664`. Этот тест ловит это compile-time через JSON.
        let arm = serde_json::to_value(HwArch::Arm64).unwrap();
        let intel = serde_json::to_value(HwArch::X8664).unwrap();
        assert_eq!(arm, "arm64");
        assert_eq!(intel, "x86_64");
    }

    #[test]
    fn report_serializes_with_snake_case() {
        let report = HwReport {
            os: HwOs::Macos,
            arch: HwArch::Arm64,
            cpu_model: "Apple M2 Pro".into(),
            ram_gb: 16,
            metal_supported: true,
            recommendation: Some(LocalEnginePreset::Balanced),
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["os"], "macos");
        assert_eq!(json["arch"], "arm64");
        assert_eq!(json["cpu_model"], "Apple M2 Pro");
        assert_eq!(json["ram_gb"], 16);
        assert_eq!(json["recommendation"], "balanced");
    }

    #[test]
    fn report_recommendation_serializes_null_on_non_mac() {
        let report = HwReport {
            os: HwOs::Linux,
            arch: HwArch::X8664,
            cpu_model: "unknown".into(),
            ram_gb: 32,
            metal_supported: false,
            recommendation: None,
        };
        let json = serde_json::to_value(&report).unwrap();
        assert!(json["recommendation"].is_null());
    }

    #[test]
    fn zero_ram_returns_none_recommendation() {
        // В sandbox / контейнере sysctl может не вернуть hw.memsize.
        // Probe выдаёт `ram_gb=0` → recommendation=None, UI обязан показать
        // «не удалось определить железо» вместо тихого Light fallback'а.
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::Arm64, 0, true, false),
            None,
            "ram_gb=0 → None (probe failed, не выбираем preset)"
        );
        assert_eq!(
            recommend_preset(HwOs::Macos, HwArch::X8664, 0, false, false),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn probe_returns_sane_values_on_macos() {
        let r = probe_hardware();
        assert_eq!(r.os, HwOs::Macos);
        if r.ram_gb == 0 {
            // Sandboxed / контейнерный run — sysctl недоступен. Probe должен
            // консервативно отказать от рекомендации.
            assert!(
                r.recommendation.is_none(),
                "ram_gb=0 должен → recommendation=None, got: {:?}",
                r.recommendation
            );
        } else {
            // Реальный Mac — recommendation должна быть выдана и валидна.
            assert!(
                !r.cpu_model.is_empty(),
                "cpu_model не должен быть пустым при ram_gb>0"
            );
            if let Some(p) = r.recommendation {
                assert!(matches!(
                    p,
                    LocalEnginePreset::Light
                        | LocalEnginePreset::Balanced
                        | LocalEnginePreset::Quality
                ));
            }
        }
    }
}
