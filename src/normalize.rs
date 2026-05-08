use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

// NFKC, Unicode lowercase, and punctuation/whitespace collapse. Diacritics and
// non-Latin scripts are preserved. This is not transliteration or case folding.
pub(crate) fn name(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut separator = false;
    for ch in value.nfkc().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() || is_combining_mark(ch) {
            if separator && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            separator = false;
        } else {
            separator = true;
        }
    }
    out.nfkc().collect()
}

pub(crate) fn searchable(value: &str) -> bool {
    value.chars().any(char::is_alphanumeric)
}

pub(crate) fn sorted_tokens(value: &str) -> String {
    let mut tokens: Vec<_> = value.split_whitespace().collect();
    tokens.sort_unstable();
    tokens.join(" ")
}
