//! Cached CPU feature and cache topology detection.

use std::sync::OnceLock;

/// Detected CPU features and cache sizes.
///
/// # Example
///
/// ```rust
/// let features = kernelkit::cpu_features::detect();
/// assert!(features.cache_line_size > 0);
/// ```
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuFeatures {
    /// Whether AVX-512 Foundation is available.
    pub avx512: bool,
    /// Whether AVX-512 Byte/Word operations are available (required for simdsieve AVX-512).
    pub avx512bw: bool,
    /// Whether AVX-512 Vector Length extensions are available.
    pub avx512vl: bool,
    /// Whether AVX-512 Vector Byte Manipulation Instructions are available.
    pub avx512vbmi: bool,
    /// Whether AVX2 is available.
    pub avx2: bool,
    /// Whether ARM NEON is available.
    pub neon: bool,
    /// The cache line size in bytes.
    pub cache_line_size: usize,
    /// The L1 data cache size in bytes.
    pub l1_size: usize,
    /// The L2 cache size in bytes.
    pub l2_size: usize,
    /// The L3 cache size in bytes.
    pub l3_size: usize,
}

/// Detect CPU features and cache topology once, then return the cached result.
#[must_use]
pub fn detect() -> CpuFeatures {
    static FEATURES: OnceLock<CpuFeatures> = OnceLock::new();
    *FEATURES.get_or_init(detect_impl)
}

fn detect_impl() -> CpuFeatures {
    let (cache_line_size, l1_size, l2_size, l3_size) = detect_cache_sizes();

    let avx512 = detect_avx512();
    CpuFeatures {
        avx512,
        avx512bw: avx512 && detect_avx512bw(),
        avx512vl: avx512 && detect_avx512vl(),
        avx512vbmi: avx512 && detect_avx512vbmi(),
        avx2: detect_avx2(),
        neon: detect_neon(),
        cache_line_size,
        l1_size,
        l2_size,
        l3_size,
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_avx512() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_avx512() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_avx512bw() -> bool {
    std::arch::is_x86_feature_detected!("avx512bw")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_avx512bw() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_avx512vl() -> bool {
    std::arch::is_x86_feature_detected!("avx512vl")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_avx512vl() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_avx512vbmi() -> bool {
    std::arch::is_x86_feature_detected!("avx512vbmi")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_avx512vbmi() -> bool {
    false
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_avx2() -> bool {
    false
}

#[cfg(target_arch = "aarch64")]
fn detect_neon() -> bool {
    std::arch::is_aarch64_feature_detected!("neon")
}

#[cfg(target_arch = "arm")]
fn detect_neon() -> bool {
    std::arch::is_arm_feature_detected!("neon")
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
fn detect_neon() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn detect_cache_sizes() -> (usize, usize, usize, usize) {
    let line_size = read_cache_value("/sys/devices/system/cpu/cpu0/cache/index0/coherency_line_size")
        .or_else(|| read_cache_value("/sys/devices/system/cpu/cpu0/cache/index1/coherency_line_size"))
        .unwrap_or(64);
    let l1_size = probe_l1_cache_size().unwrap_or(32 * 1024);
    let l2_size = read_cache_value("/sys/devices/system/cpu/cpu0/cache/index2/size").unwrap_or(256 * 1024);
    let l3_size = read_cache_value("/sys/devices/system/cpu/cpu0/cache/index3/size").unwrap_or_default();
    (line_size, l1_size, l2_size, l3_size)
}

#[cfg(target_os = "linux")]
fn probe_l1_cache_size() -> Option<usize> {
    for index in 0..=1 {
        let type_path = format!("/sys/devices/system/cpu/cpu0/cache/index{index}/type");
        if let Ok(cache_type) = std::fs::read_to_string(&type_path) {
            let cache_type = cache_type.trim().to_ascii_lowercase();
            if cache_type == "data" || cache_type == "unified" {
                let size_path = format!("/sys/devices/system/cpu/cpu0/cache/index{index}/size");
                if let Ok(raw) = std::fs::read_to_string(&size_path) {
                    if let Ok(val) = parse_cache_value(raw.trim()) {
                        return Some(val);
                    }
                }
            }
        }
    }
    read_cache_value("/sys/devices/system/cpu/cpu0/cache/index0/size")
        .or_else(|| read_cache_value("/sys/devices/system/cpu/cpu0/cache/index1/size"))
}

#[cfg(not(target_os = "linux"))]
fn detect_cache_sizes() -> (usize, usize, usize, usize) {
    (64, 32 * 1024, 256 * 1024, 0)
}

#[cfg(target_os = "linux")]
fn read_cache_value(path: &'static str) -> Option<usize> {
    let raw = std::fs::read_to_string(path).ok()?;
    parse_cache_value(raw.trim()).ok()
}

#[cfg(target_os = "linux")]
fn parse_cache_value(raw: &str) -> Result<usize, ()> {
    let trimmed = raw.trim().to_ascii_uppercase();
    if trimmed.is_empty() {
        return Err(());
    }
    if let Some(val) = trimmed
        .strip_suffix("GIB")
        .or_else(|| trimmed.strip_suffix("GB"))
        .or_else(|| trimmed.strip_suffix('G'))
    {
        let size = val.trim().parse::<usize>().map_err(|_| ())?;
        return size
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
            .and_then(|v| v.checked_mul(1024))
            .ok_or(());
    }
    if let Some(val) = trimmed
        .strip_suffix("MIB")
        .or_else(|| trimmed.strip_suffix("MB"))
        .or_else(|| trimmed.strip_suffix('M'))
    {
        let size = val.trim().parse::<usize>().map_err(|_| ())?;
        return size
            .checked_mul(1024)
            .and_then(|v| v.checked_mul(1024))
            .ok_or(());
    }
    if let Some(val) = trimmed
        .strip_suffix("KIB")
        .or_else(|| trimmed.strip_suffix("KB"))
        .or_else(|| trimmed.strip_suffix('K'))
    {
        let size = val.trim().parse::<usize>().map_err(|_| ())?;
        return size.checked_mul(1024).ok_or(());
    }
    if let Some(val) = trimmed
        .strip_suffix("BYTES")
        .or_else(|| trimmed.strip_suffix('B'))
    {
        return val.trim().parse::<usize>().map_err(|_| ());
    }
    trimmed.parse::<usize>().map_err(|_| ())
}
#[cfg(test)]
mod tests {
    use super::detect;

    #[test]
    fn detect_returns_stable_cached_result() {
        let first = detect();
        let second = detect();
        assert_eq!(first, second);
        assert!(first.cache_line_size > 0);
        assert!(first.l1_size > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_cache_sizes_are_non_zero_for_l1_and_l2() {
        let features = detect();
        assert!(features.l1_size > 0);
        assert!(features.l2_size > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_cache_value_handles_all_unit_suffixes() {
        use super::parse_cache_value;
        assert_eq!(parse_cache_value("32K"), Ok(32 * 1024));
        assert_eq!(parse_cache_value("32KB"), Ok(32 * 1024));
        assert_eq!(parse_cache_value("32 KiB"), Ok(32 * 1024));
        assert_eq!(parse_cache_value("256M"), Ok(256 * 1024 * 1024));
        assert_eq!(parse_cache_value("1G"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_cache_value("1 GiB"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_cache_value("64 B"), Ok(64));
        assert_eq!(parse_cache_value("64BYTES"), Ok(64));
        assert_eq!(parse_cache_value("4096"), Ok(4096));
        assert!(parse_cache_value("invalid").is_err());
        assert!(parse_cache_value("").is_err());
    }
}
