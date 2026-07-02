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

/// [ctx-fix] Реальное ctx-окно sidecar'а = `llm::DEFAULT_CTX_SIZE` (8192),
/// фиксировано `--ctx-size` для ВСЕХ presets (разные модели, одинаковый ctx).
/// llama.cpp роняет sidecar (exit 1) если один только prompt > ctx-4 (~8188).
///
/// Резерв под system prompt (expert/main ~1.8K) + output (MAIN_MAX_TOKENS 1536)
/// + служебные токены. Остаток = потолок transcript'а для single-pass.
const PROMPT_OVERHEAD_TOKENS: usize = 3_400;

/// Максимум токенов transcript'а для single-pass (иначе → map-reduce).
/// 8192 − 3400 ≈ 4790, округляем вниз с запасом.
const SINGLE_PASS_MAX_TOKENS: usize = 4_600;

/// Целевой размер одного chunk'а (map-call) в токенах. Ниже single-pass —
/// оставляем место map/reduce system-промпту + выводу.
const MAX_TOKENS_PER_CHUNK: usize = 3_200;

/// Continuity-overlap хвоста предыдущего chunk'а, в символах (≈256 токенов).
const OVERLAP_CHARS: usize = 1_024;

const _: () = assert!(SINGLE_PASS_MAX_TOKENS + PROMPT_OVERHEAD_TOKENS <= 8_192);

/// Конфигурация chunk window'а. [ctx-fix] Единицы — ТОКЕНЫ (оценка через
/// `estimate_tokens`), не chars: старая char-эвристика (4 chars/token)
/// недооценивала кириллицу ~2× → русский transcript «влезал» под char-порог,
/// но давал 9K токенов и ронял sidecar overflow'ом.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChunkConfig {
    /// Максимальный размер одного chunk'а в ТОКЕНАХ (est.).
    pub max_tokens: usize,
    /// Сколько последних chars предыдущего chunk'а добавить к началу следующего.
    pub overlap_chars: usize,
    /// Если transcript (в токенах) не больше — single-pass, без chunking.
    pub trigger_tokens: usize,
}

impl ChunkConfig {
    /// [ctx-fix] Все presets сейчас запускаются с одним ctx (8192), поэтому
    /// token-бюджеты preset-независимы. `match` оставлен как точка расширения
    /// на случай per-preset ctx в будущем (тогда Balanced/Quality получат
    /// больший `--ctx-size` и, соответственно, больше `trigger_tokens`).
    pub(crate) fn for_preset(preset: LocalEnginePreset) -> Self {
        match preset {
            LocalEnginePreset::Light
            | LocalEnginePreset::Balanced
            | LocalEnginePreset::Quality => Self {
                max_tokens: MAX_TOKENS_PER_CHUNK,
                overlap_chars: OVERLAP_CHARS,
                trigger_tokens: SINGLE_PASS_MAX_TOKENS,
            },
        }
    }
}

/// [ctx-fix] Оценка токенов по UTF-8 БАЙТАМ (÷4), не по chars. Байты
/// коррелируют с BPE-токенами устойчивее по разным алфавитам:
///   - ASCII/латиница: ~4 байта/char, ~4 char/token → ≈ реальным токенам.
///   - Кириллица: 2 байта/char, Qwen ~2 char/token → ~4 байта/token → тоже ≈.
///
/// Слегка консервативна для кириллицы (лучше разрезать раньше, чем overflow).
pub(crate) fn estimate_tokens(transcript_md: &str) -> usize {
    transcript_md.len() / 4
}

/// Нужно ли chunking: оценка токенов transcript'а превышает single-pass потолок.
pub(crate) fn needs_chunking(transcript_md: &str, cfg: &ChunkConfig) -> bool {
    estimate_tokens(transcript_md) > cfg.trigger_tokens
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
    // Short circuit для коротких inputs (по оценке токенов, не chars).
    if estimate_tokens(transcript_md) <= cfg.max_tokens {
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

    // Greedy pack блоков в chunk'и ≤ max_tokens (оценка по байтам).
    let mut chunks: Vec<String> = Vec::new();
    let mut current_chunk = String::new();
    for block in blocks {
        let block_len = estimate_tokens(&block);
        let cur_len = estimate_tokens(&current_chunk);
        // Если добавление этого block'а превысит max_tokens И current_chunk
        // непустой — flush, начнём следующий chunk с overlap.
        if !current_chunk.is_empty() && cur_len + block_len + 1 > cfg.max_tokens {
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
/// [ctx-fix] `max_chars` выводится из token-бюджета (~4 байта/token); для
/// broken-format (обычно ASCII/латиница) байты ≈ chars, так что порог держит
/// chunk ≈ `max_tokens`.
fn chunk_by_chars_fallback(transcript_md: &str, cfg: &ChunkConfig) -> Vec<String> {
    let max_chars = cfg.max_tokens.saturating_mul(4).max(1);
    let step = max_chars.saturating_sub(cfg.overlap_chars).max(1);
    let mut chunks: Vec<String> = Vec::new();
    let mut start_char = 0usize;
    let total = transcript_md.chars().count();
    while start_char < total {
        let end_char = (start_char + max_chars).min(total);
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
        let cfg = light_cfg(); // trigger_tokens = SINGLE_PASS_MAX_TOKENS
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
        // Каждый chunk ≤ token-бюджет в chars (fallback: max_tokens*4).
        for c in &chunks {
            assert!(
                c.chars().count() <= cfg.max_tokens * 4,
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
            // (max_tokens*4 chars + overlap_chars worst-case).
            assert!(
                c.chars().count() <= cfg.max_tokens * 4 + cfg.overlap_chars,
                "chunk {i} too large: {} chars",
                c.chars().count()
            );
        }
    }
}
