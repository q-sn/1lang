//! Word-level diff between a text and its corrected version, and linking of
//! the model's explanations to the edits. Positions reported by LLMs are
//! unreliable, so the edits are always computed here.

use crate::types::{CheckIssue, CheckSegment};

/// Above this many token pairs the whole text becomes one edit.
const MAX_CELLS: usize = 4_000_000;

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Word,
    Space,
    Other,
}

fn class(c: char) -> Class {
    if c.is_alphanumeric() {
        Class::Word
    } else if c.is_whitespace() {
        Class::Space
    } else {
        Class::Other
    }
}

/// Words (with inner apostrophes and hyphens), whitespace runs, single other characters.
fn tokenize(s: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        let cl = class(c);
        let mut j = i + 1;
        if cl != Class::Other {
            while j < chars.len() {
                let (_, d) = chars[j];
                let joiner = cl == Class::Word
                    && matches!(d, '\'' | '’' | '-')
                    && chars.get(j + 1).is_some_and(|(_, n)| n.is_alphanumeric());
                if class(d) == cl || joiner {
                    j += 1;
                } else {
                    break;
                }
            }
        }
        let end = chars.get(j).map_or(s.len(), |(b, _)| *b);
        out.push(&s[start..end]);
        i = j;
    }
    out
}

/// Splits `original` into unchanged pieces and edits that turn it into `corrected`.
pub fn segments(original: &str, corrected: &str) -> Vec<CheckSegment> {
    let a = tokenize(original);
    let b = tokenize(corrected);

    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);

    // (is_equal, original part, corrected part) per token step.
    let mut steps: Vec<(bool, &str, &str)> = a[..prefix].iter().map(|t| (true, *t, *t)).collect();
    if (am.len() + 1) * (bm.len() + 1) > MAX_CELLS {
        steps.extend(am.iter().map(|t| (false, *t, "")));
        steps.extend(bm.iter().map(|t| (false, "", *t)));
    } else {
        steps.extend(lcs_steps(am, bm));
    }
    steps.extend(a[a.len() - suffix..].iter().map(|t| (true, *t, *t)));

    let mut segs: Vec<CheckSegment> = vec![];
    let mut last_equal: Option<bool> = None;
    for (eq, o, c) in steps {
        match segs.last_mut() {
            Some(s) if last_equal == Some(eq) => {
                s.original.push_str(o);
                s.corrected.push_str(c);
            }
            _ => segs.push(CheckSegment { original: o.into(), corrected: c.into() }),
        }
        last_equal = Some(eq);
    }
    merge_close_edits(segs)
}

fn lcs_steps<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(bool, &'a str, &'a str)> {
    let (n, m) = (a.len(), b.len());
    // len[i][j] = LCS of a[i..] and b[j..]
    let mut len = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            len[at(i, j)] =
                if a[i] == b[j] { len[at(i + 1, j + 1)] + 1 } else { len[at(i + 1, j)].max(len[at(i, j + 1)]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = vec![];
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((true, a[i], b[j]));
            i += 1;
            j += 1;
        } else if j == m || (i < n && len[at(i + 1, j)] >= len[at(i, j + 1)]) {
            out.push((false, a[i], ""));
            i += 1;
        } else {
            out.push((false, "", b[j]));
            j += 1;
        }
    }
    out
}

fn is_replacement(s: &CheckSegment) -> bool {
    s.is_edit() && !s.original.is_empty() && !s.corrected.is_empty()
}

/// "want to inform" → "wanted to let you know" reads as one edit, not three.
/// A short word joins only two replacements: "bugs in" + inserted "the" stay apart.
fn is_glue(s: &str, prev: &CheckSegment, next: &CheckSegment) -> bool {
    if s.contains('\n') || !prev.is_edit() || !next.is_edit() {
        return false;
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    match words.as_slice() {
        [] => true,
        [w] => {
            w.chars().count() <= 3
                && w.chars().all(char::is_alphabetic)
                && is_replacement(prev)
                && is_replacement(next)
        }
        _ => false,
    }
}

fn merge_close_edits(segs: Vec<CheckSegment>) -> Vec<CheckSegment> {
    let mut out: Vec<CheckSegment> = vec![];
    let mut it = segs.into_iter().peekable();
    while let Some(seg) = it.next() {
        let between = !seg.is_edit()
            && match (out.last(), it.peek()) {
                (Some(prev), Some(next)) => is_glue(&seg.original, prev, next),
                _ => false,
            };
        if between {
            let next = it.next().unwrap();
            let prev = out.last_mut().unwrap();
            prev.original.push_str(&seg.original);
            prev.original.push_str(&next.original);
            prev.corrected.push_str(&seg.corrected);
            prev.corrected.push_str(&next.corrected);
        } else {
            out.push(seg);
        }
    }
    trim_edits(out)
}

/// Whitespace both sides of an edit share belongs to the unchanged text around it.
fn trim_edits(segs: Vec<CheckSegment>) -> Vec<CheckSegment> {
    fn push_equal(out: &mut Vec<CheckSegment>, s: &str) {
        if s.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if !last.is_edit() => {
                last.original.push_str(s);
                last.corrected.push_str(s);
            }
            _ => out.push(CheckSegment { original: s.into(), corrected: s.into() }),
        }
    }
    let mut out: Vec<CheckSegment> = vec![];
    for seg in segs {
        if !is_replacement(&seg) {
            if seg.is_edit() {
                out.push(seg);
            } else {
                push_equal(&mut out, &seg.original);
            }
            continue;
        }
        let (o, c) = (seg.original.as_str(), seg.corrected.as_str());
        let lead = common_ws(o.chars(), c.chars());
        let trail = common_ws(o[lead..].chars().rev(), c[lead..].chars().rev());
        push_equal(&mut out, &o[..lead]);
        out.push(CheckSegment {
            original: o[lead..o.len() - trail].into(),
            corrected: c[lead..c.len() - trail].into(),
        });
        push_equal(&mut out, &o[o.len() - trail..]);
    }
    out
}

/// Byte length of the whitespace both iterators start with.
fn common_ws(a: impl Iterator<Item = char>, b: impl Iterator<Item = char>) -> usize {
    a.zip(b).take_while(|(x, y)| x == y && x.is_whitespace()).map(|(x, _)| x.len_utf8()).sum()
}

/// Byte ranges of each segment in the original and the corrected text.
fn ranges(segs: &[CheckSegment]) -> Vec<((usize, usize), (usize, usize))> {
    let (mut o, mut c) = (0, 0);
    segs.iter()
        .map(|s| {
            let r = ((o, o + s.original.len()), (c, c + s.corrected.len()));
            o += s.original.len();
            c += s.corrected.len();
            r
        })
        .collect()
}

fn find_from(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let needle = needle.trim();
    if needle.is_empty() {
        return None;
    }
    hay.get(from..).and_then(|h| h.find(needle)).map(|p| p + from).or_else(|| hay.find(needle))
}

fn overlaps(edit: (usize, usize), span: (usize, usize)) -> bool {
    if edit.0 == edit.1 {
        span.0 <= edit.0 && edit.0 <= span.1
    } else {
        edit.0 < span.1 && span.0 < edit.1
    }
}

/// Sets `issue.segments` to the edits each issue talks about.
pub fn link_issues(issues: &mut [CheckIssue], segs: &[CheckSegment], original: &str, corrected: &str) {
    let rs = ranges(segs);
    let (mut from_o, mut from_c) = (0, 0);
    for issue in issues.iter_mut() {
        let link = |span: (usize, usize), in_original: bool| -> Vec<u32> {
            segs.iter()
                .zip(&rs)
                .enumerate()
                .filter(|(_, (s, (ro, rc)))| s.is_edit() && overlaps(if in_original { *ro } else { *rc }, span))
                .map(|(i, _)| i as u32)
                .collect()
        };
        let mut found = vec![];
        if let Some(p) = find_from(original, &issue.original, from_o) {
            let span = (p, p + issue.original.trim().len());
            found = link(span, true);
            from_o = span.1;
        }
        if found.is_empty() {
            if let Some(p) = find_from(corrected, &issue.correction, from_c) {
                let span = (p, p + issue.correction.trim().len());
                found = link(span, false);
                from_c = span.1;
            }
        }
        issue.segments = found;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edits(o: &str, c: &str) -> Vec<(String, String)> {
        segments(o, c).into_iter().filter(|s| s.is_edit()).map(|s| (s.original, s.corrected)).collect()
    }

    fn join(segs: &[CheckSegment]) -> (String, String) {
        (segs.iter().map(|s| s.original.as_str()).collect(), segs.iter().map(|s| s.corrected.as_str()).collect())
    }

    #[test]
    fn segments_rebuild_both_texts() {
        let o = "Hi team, I want to inform that the release will be delay on two days.";
        let c = "Hi team, I wanted to let you know that the release will be delayed by two days.";
        let segs = segments(o, c);
        assert_eq!(join(&segs), (o.to_string(), c.to_string()));
    }

    #[test]
    fn close_edits_are_merged() {
        let e = edits("I want to inform that it will be delay on two days", "I wanted to let you know that it will be delayed by two days");
        assert_eq!(e, vec![
            ("want to inform".into(), "wanted to let you know".into()),
            ("delay on".into(), "delayed by".into()),
        ]);
    }

    #[test]
    fn insertion_and_unchanged() {
        assert_eq!(edits("in payment.", "in the payment."), vec![("".into(), "the ".into())]);
        assert!(edits("All good.", "All good.").is_empty());
        assert_eq!(edits("don't", "doesn't"), vec![("don't".into(), "doesn't".into())]);
    }

    #[test]
    fn issues_link_to_edits() {
        let o = "We have found a critical bugs in payment.";
        let c = "We found a critical bug in the payment.";
        let segs = segments(o, c);
        let mut issues = vec![
            CheckIssue { original: "a critical bugs".into(), correction: "a critical bug".into(), ..Default::default() },
            CheckIssue { original: "".into(), correction: "the payment".into(), ..Default::default() },
            CheckIssue { original: "nowhere".into(), correction: "nothing".into(), ..Default::default() },
        ];
        link_issues(&mut issues, &segs, o, c);
        let text = |i: u32| segs[i as usize].corrected.trim().to_string();
        assert_eq!(issues[0].segments.iter().map(|&i| text(i)).collect::<Vec<_>>(), vec!["bug"]);
        assert_eq!(issues[1].segments.iter().map(|&i| text(i)).collect::<Vec<_>>(), vec!["the"]);
        assert!(issues[2].segments.is_empty());
    }
}
