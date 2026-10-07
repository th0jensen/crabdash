//! Raw command output and byte/string conversions; parsing belongs to domain modules.

#[derive(Debug, Clone)]
pub struct Output(pub Vec<u8>);

impl Output {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<u8>> for Output {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<String> for Output {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}

impl From<Output> for String {
    fn from(o: Output) -> Self {
        String::from_utf8_lossy(&o.0).trim().to_string()
    }
}

impl From<Output> for Vec<u8> {
    fn from(o: Output) -> Self {
        o.0
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.0))
    }
}

impl AsRef<[u8]> for Output {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::ops::Deref for Output {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}
