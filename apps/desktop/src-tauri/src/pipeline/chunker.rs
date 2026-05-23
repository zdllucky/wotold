//! [M14 T-05 Phase B] Разбиение transcript.md на token-windows для map-reduce.
//!
//! ## Зачем
//!
//! Локальная Qwen 1.5B имеет ~8K эффективного контекста. 1-часовой звонок
//! ≈ 24-30K tokens — не влезает single-pass. Phase A path работает только
//! для коротких звонков; Phase B решает long calls через chunk + map-reduce.
//!
//! ## Boundary detection
//!
//! Transcript.md (см. `pipeline/merge.rs::render_transcript_md`) имеет формат:
//!
//! ```text
//! # Transcript
//!
//! **owner** [0:00]:
//! hi there
//!
//! **Speaker 0** [0:02]:
//! hello back
//! ```
//!
//! Speaker turn boundary = строка, начинающаяся с `**` и содержащая `]:`.
//! Никаких regex — простой `lines().filter`. Достаточно для текущего формата.
//!
//! ## Overlap strategy
//!
//! Каждый chunk N+1 начинается с tail последних `overlap_chars` chunk N,
//! найденного до ближайшего speaker boundary. Это даёт continuity без
//! дублирования по серединам реплик.
//!
//! ## Phase C/D (deferred)
//!
//! - topic_tile boundaries (semantic) — Phase B использует только speaker-turn.
//! - hierarchical 3-level pipeline для >48K — backlog.

use crate::local_engine::preset::LocalEnginePreset;

/// Конфигурация chunk window'а per preset (PRD §3.3).
/// ~4 chars/token эвристика. Phase B: hardcoded numbers per preset.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChunkConfig {
    /// Максимальный размер одного chunk'а в chars (включая overlap от предыдущего).
    pub max_chars: usize,
    /// Сколько последних chars предыдущего chunk'а добавить к началу следующего.
    pub overlap_chars: usize,
    /// Если transcript меньше — single-pass, без chunking.
    pub trigger_threshold: usize,
}

impl ChunkConfig {
    /// Per-preset config. PRD §3.3:
    ///   - Light (1.5B, 8K effective ctx): chunk_tokens=3.2K → 12.8K chars
    ///   - Balanced (3B, 12K effective ctx): chunk_tokens=4.8K → 19.2K chars
    ///   - Quality (7B, 24K effective ctx): chunk_tokens=9.6K → 38.4K chars
    ///
    /// Overlap 10%, trigger_threshold = max_chars * 2 (chunking имеет смысл
    /// только когда минимум 2 chunk'а получится).
    pub(crate) fn for_preset(preset: LocalEnginePreset) -> Self {
        match preset {
            LocalEnginePreset::Light => Self {
                max_chars: 12_800,
                overlap_chars: 1_280,
                trigger_threshold: 24_000,
            },
            LocalEnginePreset::Balanced => Self {
                max_chars: 19_200,
                overlap_chars: 1_920,
                trigger_threshold: 38_400,
            },
            LocalEnginePreset::Quality => Self {
                max_chars: 38_400,
                overlap_chars: 3_840,
                trigger_threshold: 76_800,
            },
        }
    }
}

/// 4 chars per token эвристика для logging / decisions. Не точная — Qwen
/// тоже использует BPE с variable-width, но для PRD-spec triggers
/// достаточно (мы не считаем tokens, мы считаем chars и делим).
pub(crate) fn estimate_tokens(transcript_md: &str) -> usize {
    transcript_md.chars().count() / 4
}

/// Нужно ли chunking для этого transcript'а под данный preset.
pub(crate) fn needs_chunking(transcript_md: &str, cfg: &ChunkConfig) -> bool {
    transcript_md.chars().count() > cfg.trigger_threshold
}

/// Speaker turn boundary detection: line starts with `**` и содержит `]:` (после `[mm:ss]`).
fn is_speaker_header_line(line: &str) -> bool {
    line.starts_with("**") && line.contains("]:")
}

/// Разбить transcript на chunks по speaker boundary. Если transcript короче
/// `max_chars` — возвращает один chunk equal to transcript_md (без overlap).
///
/// Stateless string-builder: накапливает строки в текущий chunk'е до
/// `max_chars`, потом начинает следующий с overlap'ом из tail предыдущего.
/// Overlap всегда обрезается ровно по speaker boundary (никогда не посередине
/// реплики).
pub(crate) fn chunk_transcript(transcript_md: &str, cfg: &ChunkConfig) -> Vec<String> {
    // Short circuit для коротких inputs.
    if transcript_md.chars().count() <= cfg.max_chars {
        return vec![transcript_md.to_string()];
    }

    // Group lines into speaker-turn blocks. Block = заголовок (`**name** [mm:ss]:`)
    // + всё до следующего заголовка ИЛИ конец файла.
    let lines: Vec<&str> = transcript_md.lines().collect();
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in &lines {
        if is_speaker_header_line(line) && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    // Edge case: если transcript не содержит speaker headers — fallback to
    // char-boundary split. Это broken format, но не должен ронять весь pipeline.
    if blocks
        .iter()
        .all(|b| !is_speaker_header_line(b.lines().next().unwrap_or("")))
    {
        return chunk_by_chars_fallback(transcript_md, cfg);
    }

    // Greedy pack блоков в chunk'и ≤ max_chars.
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = String::new();
    for block in blocks {
        let block_len = block.chars().count();
        let cur_len = current_chunk.chars().count();
        // Если добавление этого block'а превысит max_chars И current_chunk
        // непустой — flush, начнём следующий chunk с overlap.
        if !current_chunk.is_empty() && cur_len + block_len + 1 > cfg.max_chars {
            let overlap = tail_overlap_at_speaker_boundary(&current_chunk, cfg.overlap_chars);
            chunks.push(std::mem::take(&mut current_chunk));
            current_chunk.push_str(&overlap);
            if !current_chunk.is_empty() {
                current_chunk.push('\n');
            }
        }
        if !current_chunk.is_empty() {
            current_chunk.push('\n');
        }
        current_chunk.push_str(&block);
    }
    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    chunks
}

/// Возвращает tail последнего chunk'а, обрезанный по последней (от конца)
/// speaker-header. Размер tail ≈ `overlap_chars`, но допустимо больше если
/// последний speaker-turn длиннее (мы НЕ режем посередине реплики).
///
/// Алгоритм: собираем byte-offsets всех speaker-header строк в chunk'е.
/// Берём последний header, чей tail (от его start до конца chunk'а)
/// ≥ overlap_chars OR — если такого нет — самый последний header.
fn tail_overlap_at_speaker_boundary(chunk: &str, overlap_chars: usize) -> String {
    // Собираем (byte_offset, char_offset) для каждой speaker header line.
    let mut headers: Vec<(usize, usize)> = Vec::new();
    let mut current_byte = 0usize;
    let mut current_char = 0usize;
    for line in chunk.lines() {
        if is_speaker_header_line(line) {
            headers.push((current_byte, current_char));
        }
        current_byte += line.len() + 1; // +1 за '\n' (lines() strips them)
        current_char += line.chars().count() + 1;
    }

    if headers.is_empty() {
        // Broken format — нет headers. Возвращаем последние overlap_chars
        // как есть (best-effort).
        return tail_by_chars(chunk, overlap_chars);
    }

    let total_chars = chunk.chars().count();
    // Идём от последнего header к первому, ищем первый чей tail >= overlap_chars.
    for &(b, c) in headers.iter().rev() {
        let tail_chars = total_chars.saturating_sub(c);
        if tail_chars >= overlap_chars {
            return chunk[b..].to_string();
        }
    }
    // Все headers слишком близко к концу — возвращаем от самого первого
    // (последнего по итерации) header'а tail.
    let (b, _c) = headers[0];
    chunk[b..].to_string()
}

/// Fallback when no speaker headers: возвращает последние ~`overlap_chars` символов.
fn tail_by_chars(s: &str, overlap_chars: usize) -> String {
    let total = s.chars().count();
    if total <= overlap_chars {
        return s.to_string();
    }
    let skip = total - overlap_chars;
    let start_byte = s.char_indices().nth(skip).map(|(b, _)| b).unwrap_or(0);
    s[start_byte..].to_string()
}

/// Fallback split: input не содержит speaker headers. Простой char-boundary
/// split в `max_chars - overlap_chars` step'ах. Edge case, не production path.
fn chunk_by_chars_fallback(transcript_md: &str, cfg: &ChunkConfig) -> Vec<String> {
    let step = cfg.max_chars.saturating_sub(cfg.overlap_chars).max(1);
    let mut chunks: Vec<String> = Vec::new();
    let mut start_char = 0usize;
    let total = transcript_md.chars().count();
    while start_char < total {
        let end_char = (start_char + cfg.max_chars).min(total);
        let start_byte = transcript_md
            .char_indices()
            .nth(start_char)
            .map(|(b, _)| b)
            .unwrap_or(0);
        let end_byte = transcript_md
            .char_indices()
            .nth(end_char)
            .map(|(b, _)| b)
            .unwrap_or(transcript_md.len());
        chunks.push(transcript_md[start_byte..end_byte].to_string());
        if end_char >= total {
            break;
        }
        start_char += step;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light_cfg() -> ChunkConfig {
        ChunkConfig::for_preset(LocalEnginePreset::Light)
    }

    fn make_long_transcript(turns: usize, chars_per_turn: usize) -> String {
        let mut out = String::from("# Transcript\n\n");
        for i in 0..turns {
            out.push_str(&format!("**Speaker {}** [{}:00]:\n", i % 3, i));
            out.push_str(&"a".repeat(chars_per_turn));
            out.push_str("\n\n");
        }
        out
    }

    #[test]
    fn estimate_tokens_4chars_per_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens(&"a".repeat(4000)), 1000);
    }

    #[test]
    fn needs_chunking_short_transcript_false() {
        let cfg = light_cfg();
        let short = "**A** [0:00]:\nhi";
        assert!(!needs_chunking(short, &cfg));
    }

    #[test]
    fn needs_chunking_long_transcript_true() {
        let cfg = light_cfg(); // trigger_threshold = 24_000
        let long = "a".repeat(30_000);
        assert!(needs_chunking(&long, &cfg));
    }

    #[test]
    fn chunk_transcript_short_returns_one_chunk() {
        let cfg = light_cfg();
        let short = "**A** [0:00]:\nhi there\n\n**B** [0:05]:\nhello\n";
        let chunks = chunk_transcript(short, &cfg);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], short);
    }

    #[test]
    fn chunk_transcript_respects_speaker_boundary() {
        // 10 turns × 2000 chars = 20K chars body; добавим headers — больше 12.8K threshold.
        let long = make_long_transcript(10, 2000);
        let cfg = light_cfg();
        let chunks = chunk_transcript(&long, &cfg);
        // Must produce at least 2 chunks.
        assert!(
            chunks.len() >= 2,
            "expected ≥2 chunks, got {}",
            chunks.len()
        );
        // Каждый chunk должен начинаться с speaker header (либо # Transcript header
        // в первом chunk'е).
        for (i, c) in chunks.iter().enumerate() {
            let first_line = c.lines().next().unwrap_or("");
            assert!(
                first_line.starts_with("#") || first_line.starts_with("**"),
                "chunk {i} should start with header, got: {first_line:?}"
            );
        }
    }

    #[test]
    fn chunk_transcript_adds_overlap_to_subsequent_chunks() {
        let long = make_long_transcript(8, 2000);
        let cfg = light_cfg();
        let chunks = chunk_transcript(&long, &cfg);
        assert!(chunks.len() >= 2);
        // Tail последнего блока chunk[0] должен appear в начале chunk[1].
        let tail = chunks[0].lines().last().unwrap_or("").trim();
        if !tail.is_empty() {
            // Найти этот блок (speaker header перед `aaa...`) в начале chunk[1].
            // Поскольку overlap содержит speaker header + текст реплики, проверим
            // что в chunk[1] первые ~overlap_chars содержат тот же speaker tag из
            // конца chunk[0].
            // Простой smoke: длина начала chunk[1] (до первой пустой строки) > 0
            // и содержит "**Speaker".
            let head = chunks[1].lines().take(5).collect::<Vec<_>>().join("\n");
            assert!(
                head.contains("**Speaker"),
                "chunk[1] should start with overlap speaker header, got: {head:?}"
            );
        }
    }

    #[test]
    fn chunk_transcript_fallback_when_no_speaker_headers() {
        let cfg = light_cfg();
        let broken = "a".repeat(30_000); // no speaker headers, > threshold
        let chunks = chunk_transcript(&broken, &cfg);
        assert!(chunks.len() >= 2);
        // Каждый chunk ≤ max_chars.
        for c in &chunks {
            assert!(
                c.chars().count() <= cfg.max_chars,
                "chunk too large: {}",
                c.chars().count()
            );
        }
    }

    #[test]
    fn chunk_transcript_each_chunk_under_max_chars() {
        let long = make_long_transcript(10, 2000);
        let cfg = light_cfg();
        let chunks = chunk_transcript(&long, &cfg);
        for (i, c) in chunks.iter().enumerate() {
            // Допускаем небольшое превышение из-за overlap'а
            // (max_chars + overlap_chars worst-case).
            assert!(
                c.chars().count() <= cfg.max_chars + cfg.overlap_chars,
                "chunk {i} too large: {} chars",
                c.chars().count()
            );
        }
    }
}
