pub const CODE_BYTES_PER_TOKEN_TENTHS: usize = 36;
pub const PROSE_BYTES_PER_TOKEN: usize = 4;

pub fn code(bytes: usize) -> usize {
    (bytes * 10).div_ceil(CODE_BYTES_PER_TOKEN_TENTHS)
}

pub fn prose(bytes: usize) -> usize {
    bytes.div_ceil(PROSE_BYTES_PER_TOKEN)
}

pub fn code_text(text: &str) -> usize {
    code(text.len())
}

pub fn prose_text(text: &str) -> usize {
    prose(text.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Code,
    Prose,
}

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub text: String,
    pub code_bytes: usize,
    pub prose_bytes: usize,
}

impl Part {
    pub fn push(&mut self, piece: &str, kind: Kind) {
        self.text.push_str(piece);
        match kind {
            Kind::Code => self.code_bytes += piece.len(),
            Kind::Prose => self.prose_bytes += piece.len(),
        }
    }

    pub fn tokens_with(&self, piece: &str, kind: Kind) -> usize {
        match kind {
            Kind::Code => code(self.code_bytes + piece.len()) + prose(self.prose_bytes),
            Kind::Prose => code(self.code_bytes) + prose(self.prose_bytes + piece.len()),
        }
    }

    pub fn tokens(&self) -> usize {
        code(self.code_bytes) + prose(self.prose_bytes)
    }
}
