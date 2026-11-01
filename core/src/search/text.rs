pub const STOP: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "and", "or", "is", "it", "be", "with", "by", "at", "as",
    "this", "that", "from", "my", "me", "we", "i", "how", "do", "can", "into", "some", "all", "add", "make",
    "create", "implement", "write", "new", "need", "want", "use", "using", "please", "should",
];

pub fn split_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev: Option<char> = None;
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            prev = None;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let boundary = match prev {
            Some(p) => {
                (p.is_lowercase() && c.is_uppercase())
                    || (p.is_alphabetic() && c.is_ascii_digit())
                    || (p.is_ascii_digit() && c.is_alphabetic())
                    || (p.is_uppercase() && c.is_uppercase() && next.is_some_and(|n| n.is_lowercase()))
            }
            None => false,
        };
        if boundary && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        for lower in c.to_lowercase() {
            current.push(lower);
        }
        prev = Some(c);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

pub fn stem(word: &str) -> String {
    if word.len() > 4 && word.ends_with("ies") {
        return format!("{}y", &word[..word.len() - 3]);
    }
    if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") && !word.ends_with("us") {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

pub fn query_terms(query: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for word in split_words(query) {
        if word.chars().count() < 2 || STOP.contains(&word.as_str()) {
            continue;
        }
        if !seen.contains(&word) {
            seen.push(word);
        }
        if seen.len() >= 12 {
            break;
        }
    }
    seen
}

pub fn fts_or(terms: &[String]) -> Option<String> {
    if terms.is_empty() {
        return None;
    }
    let quoted: Vec<String> = terms.iter().map(|t| format!("\"{}\"", t.replace('"', ""))).collect();
    Some(quoted.join(" OR "))
}

pub fn trigram_terms(query: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for raw in query.split_whitespace() {
        let trimmed = raw.trim_matches(|c: char| !(c.is_alphanumeric() || c == '.' || c == '/' || c == '-' || c == '_'));
        let lower = trimmed.to_lowercase();
        if lower.chars().count() >= 3 && !STOP.contains(&lower.as_str()) && !seen.contains(&lower) {
            seen.push(lower);
        }
    }
    for word in query_terms(query) {
        if word.chars().count() >= 4 && !seen.contains(&word) {
            seen.push(word);
        }
    }
    seen.truncate(12);
    seen
}
