//! Text recognition with the built-in Windows OCR (Windows.Media.Ocr).

use image::RgbaImage;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrLine, OcrResult};
use windows::Storage::Streams::DataWriter;

use crate::{Error, OcrOutput, Result};

/// BCP-47 tags of installed OCR languages.
pub fn available_languages() -> Result<Vec<String>> {
    let langs = OcrEngine::AvailableRecognizerLanguages()?;
    let mut out = vec![];
    for l in langs {
        out.push(l.LanguageTag()?.to_string());
    }
    Ok(out)
}

fn to_bitmap(img: &RgbaImage) -> Result<SoftwareBitmap> {
    let mut bgra = img.as_raw().clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra)?;
    let buffer = writer.DetachBuffer()?;
    Ok(SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        img.width() as i32,
        img.height() as i32,
        BitmapAlphaMode::Premultiplied,
    )?)
}

/// OCR works best when glyphs are ~20–40 px tall; screen text is often smaller.
fn prepare(img: &RgbaImage) -> RgbaImage {
    let max = OcrEngine::MaxImageDimension().unwrap_or(2600);
    let (w, h) = (img.width(), img.height());
    let mut scale = if h < 600 && w < 1300 { 2.0 } else { 1.0 };
    let longest = (w.max(h) as f64) * scale;
    if longest > max as f64 {
        scale *= max as f64 / longest;
    }
    if (scale - 1.0).abs() < 0.01 {
        return img.clone();
    }
    let nw = ((w as f64) * scale).round().max(1.0) as u32;
    let nh = ((h as f64) * scale).round().max(1.0) as u32;
    image::imageops::resize(img, nw, nh, image::imageops::FilterType::CatmullRom)
}

struct LineBox {
    text: String,
    x: f32,
    y: f32,
    h: f32,
}

fn line_box(line: &OcrLine) -> Result<LineBox> {
    let words = line.Words()?;
    let (mut x0, mut y0, mut y1) = (f32::MAX, f32::MAX, f32::MIN);
    for w in &words {
        let r = w.BoundingRect()?;
        x0 = x0.min(r.X);
        y0 = y0.min(r.Y);
        y1 = y1.max(r.Y + r.Height);
    }
    Ok(LineBox { text: line.Text()?.to_string(), x: x0, y: y0, h: (y1 - y0).max(1.0) })
}

fn is_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x9FFF | 0xAC00..=0xD7AF))
}

/// Join OCR lines into paragraphs: lines close to each other and similarly
/// indented belong to the same paragraph (better for translation than hard breaks).
fn join_lines(result: &OcrResult) -> Result<String> {
    let lines = result.Lines()?;
    let boxes: Vec<LineBox> = lines.into_iter().filter_map(|l| line_box(&l).ok()).collect();
    let mut out = String::new();
    for (i, b) in boxes.iter().enumerate() {
        if i > 0 {
            let prev = &boxes[i - 1];
            let gap = b.y - (prev.y + prev.h);
            let same_paragraph = gap < prev.h.max(b.h) * 0.75 && (b.x - prev.x).abs() < prev.h * 2.5;
            let ends_sentence = prev.text.trim_end().ends_with(['.', '!', '?', ':', '。', '！', '？']);
            if same_paragraph && !ends_sentence {
                if prev.text.ends_with('-') && !prev.text.ends_with(" -") {
                    out.pop();
                } else if !(is_cjk(&prev.text) && is_cjk(&b.text)) {
                    out.push(' ');
                }
            } else {
                out.push('\n');
            }
        }
        out.push_str(&b.text);
    }
    Ok(out)
}

fn recognize_with(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> Result<String> {
    let result = engine.RecognizeAsync(bitmap)?.join()?;
    join_lines(&result)
}

/// Plausibility of recognized text: words made of one script, few stray symbols.
fn score(text: &str) -> f64 {
    let mut good = 0usize;
    let mut total = 0usize;
    for word in text.split_whitespace() {
        let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.is_empty() {
            continue;
        }
        total += 1;
        let cyr = letters.iter().any(|c| matches!(*c as u32, 0x0400..=0x04FF));
        let lat = letters.iter().any(|c| c.is_ascii_alphabetic());
        if !(cyr && lat) {
            good += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    good as f64 / total as f64 + (text.len() as f64).ln() * 0.01
}

/// Recognize text. `language`: BCP-47 tag, or `None` to try the installed
/// recognizers that match `preferred` languages and keep the most plausible result.
pub fn recognize(img: &RgbaImage, language: Option<&str>, preferred: &[String]) -> Result<OcrOutput> {
    let prepared = prepare(img);

    if let Some(tag) = language.filter(|t| !t.is_empty() && *t != "auto") {
        let lang = Language::CreateLanguage(&HSTRING::from(tag))?;
        if !OcrEngine::IsLanguageSupported(&lang)? {
            return Err(Error::OcrLanguageMissing(tag.to_string()));
        }
        let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
        let bitmap = to_bitmap(&prepared)?;
        return Ok(OcrOutput { text: recognize_with(&engine, &bitmap)?, language: tag.to_string() });
    }

    let available = available_languages()?;
    if available.is_empty() {
        return Err(Error::NoOcrLanguages);
    }
    // Candidates: installed recognizers for the user's languages, one per base language.
    let mut candidates: Vec<String> = vec![];
    for p in preferred {
        if let Some(tag) = available.iter().find(|t| t.split('-').next() == Some(p.as_str())) {
            if !candidates.contains(tag) {
                candidates.push(tag.clone());
            }
        }
    }
    if candidates.is_empty() {
        candidates.push(available[0].clone());
    }

    // Each recognizer runs on its own thread; WinRT objects are created per thread.
    let results: Vec<(f64, OcrOutput)> = std::thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .iter()
            .take(3)
            .map(|tag| {
                let prepared = &prepared;
                scope.spawn(move || -> Result<(f64, OcrOutput)> {
                    let bitmap = to_bitmap(prepared)?;
                    let lang = Language::CreateLanguage(&HSTRING::from(tag.as_str()))?;
                    let engine = OcrEngine::TryCreateFromLanguage(&lang)?;
                    let text = recognize_with(&engine, &bitmap)?;
                    Ok((score(&text), OcrOutput { text, language: tag.clone() }))
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).filter_map(|r| r.ok()).collect()
    });
    let best = results.into_iter().max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(best.map(|(_, o)| o).unwrap_or_default())
}
