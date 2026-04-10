use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct Args(pub Vec<String>);

impl Args {
    pub fn new() -> Self {
        Args(vec![])
    }

    pub fn as_str_slice(&self) -> Vec<&str> {
        self.0.iter().map(String::as_str).collect()
    }

    pub fn push(&mut self, arg: impl Into<String>) {
        self.0.push(arg.into());
    }
}

impl From<Vec<String>> for Args {
    fn from(v: Vec<String>) -> Self {
        Self(v)
    }
}

impl From<Vec<&str>> for Args {
    fn from(v: Vec<&str>) -> Self {
        Self(v.into_iter().map(str::to_owned).collect())
    }
}

impl From<String> for Args {
    fn from(s: String) -> Self {
        Self(vec![s])
    }
}

impl Deref for Args {
    type Target = [String];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join(" "))
    }
}

impl IntoIterator for Args {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Args {
    type Item = &'a String;
    type IntoIter = std::slice::Iter<'a, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[macro_export]
macro_rules! args {
    ($($s:expr),*) => {
        Args(vec![$($s.to_owned()),*])
    };
}
