#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sign {
    pub id: u64,
    /// line number. 0-based.
    pub line: u64,
    pub name: String,
}

impl Sign {
    pub fn new(line: u64, name: String) -> Sign {
        Sign {
            // Placeholder id. Will be updated when actually get displayed.
            id: 0,
            line,
            name,
        }
    }
}

impl core::cmp::PartialEq for Sign {
    fn eq(&self, other: &Self) -> bool {
        self.line == other.line && self.name == other.name
    }
}
