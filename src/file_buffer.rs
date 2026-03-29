#[derive(Default)]
pub struct MemBuf {
    buf: Vec<String>,
}

impl std::ops::Deref for MemBuf {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl MemBuf {
    pub fn write(&mut self, v: String) {
        self.buf.push(v);
    }
    pub fn nth(&self, idx: usize) -> Option<&String> {
        self.buf.get(idx)
    }
}
