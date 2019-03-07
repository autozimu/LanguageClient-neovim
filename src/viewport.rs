use lsp_types::*;

/// Visible lines of editor.
///
/// Inclusive at start, exclusive at end. Both start aned end are 0-based.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
pub struct Viewport {
    pub start: u64,
    pub end: u64,
}

impl Viewport {
    #[allow(dead_code)]
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
