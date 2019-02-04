use lsp_types::*;

/// Visible lines of editor.
///
/// Inclusive at start, exclusive at end.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
pub struct Viewport {
    pub start: u64,
    pub end: u64,
}

impl Viewport {
    pub fn new(start: u64, end: u64) -> Self {
        Viewport { start, end }
    }

    fn contains(&self, line: u64) -> bool {
        line >= self.start && line < self.end
    }

    pub fn overlaps(&self, range: Range) -> bool {
        self.contains(range.start.line) || self.contains(range.end.line)
    }
}

impl std::ops::Sub for Viewport {
    type Output = Vec<Viewport>;

    fn sub(self, other: Viewport) -> Self::Output {
        vec![
            Self::new(self.start, other.start),
            Self::new(other.end, self.end),
        ]
    }
}

#[test]
fn test_new() {
    let viewport = Viewport::new(0, 7);
    assert_eq!(viewport.start, 0);
    assert_eq!(viewport.end, 7);
}

#[test]
fn test_overlaps() {
    let viewport = Viewport::new(2, 7);
    assert_eq!(
        viewport.overlaps(Range::new(Position::new(0, 0), Position::new(1, 10))),
        false
    );
    assert_eq!(
        viewport.overlaps(Range::new(Position::new(0, 0), Position::new(2, 0))),
        true
    );
}

#[test]
fn test_sub() {
    let v0 = Viewport::new(2, 7);
    let v1 = Viewport::new(0, 9);
    assert_eq!(v1 - v0, vec![Viewport::new(0, 2), Viewport::new(7, 9)]);
    assert_eq!(v0 - v1, vec![Viewport::new(2, 0), Viewport::new(9, 7)]);
}
