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
    pub fn new(start: u64, end: u64) -> Self {
        Viewport { start, end }
    }

    fn contains(&self, line: u64) -> bool {
        line >= self.start && line < self.end
    }

    pub fn overlaps(&self, range: Range) -> bool {
        self.contains(range.start.line) || self.contains(range.end.line)
    }

    pub fn diff(&self, v0: Option<&Viewport>) -> Vec<Viewport> {
        match v0 {
            None => vec![*self],
            Some(v0) => {
                if self.start >= v0.end || self.end <= v0.start {
                    return vec![*self];
                }

                let mut diffs = vec![];
                if self.start < v0.start {
                    diffs.push(Viewport::new(self.start, v0.start));
                }
                if self.end > v0.end {
                    diffs.push(Viewport::new(v0.end, self.end));
                }
                diffs
            }
        }
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
    assert_eq!(
        v1.diff(Some(&v0)),
        vec![Viewport::new(0, 2), Viewport::new(7, 9)]
    );
    assert_eq!(v0.diff(Some(&v1)), vec![]);

    let v0 = Viewport::new(0, 1);
    let v1 = Viewport::new(10, 11);
    assert_eq!(v1.diff(Some(&v0)), vec![v1]);
    assert_eq!(v0.diff(Some(&v1)), vec![v0]);
}
