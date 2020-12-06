use crate::types::{RootMarkers, ToUsize};
use anyhow::{anyhow, Result};
use log::*;
use lsp_types::{CodeAction, Position, TextEdit, Url};
use serde_json::json;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    str::FromStr,
};

pub fn escape_single_quote<S: AsRef<str>>(s: S) -> String {
    s.as_ref().replace("'", "''")
}

pub fn get_root_path<'a>(
    path: &'a Path,
    language_id: &str,
    root_markers: &Option<RootMarkers>,
) -> Result<&'a Path> {
    if let Some(ref root_markers) = *root_markers {
        let empty = &vec![];
        let root_markers = match *root_markers {
            RootMarkers::Array(ref arr) => arr,
            RootMarkers::Map(ref map) => map.get(language_id).unwrap_or(empty),
        };

        for marker in root_markers {
            let ret = traverse_up(path, |dir| {
                let p = dir.join(marker);
                let p = p.to_str();
                if p.is_none() {
                    return false;
                }
                let p = p.unwrap_or_default();
                match glob::glob(p) {
                    Ok(paths) => paths.count() > 0,
                    _ => false,
                }
            });

            if ret.is_ok() {
                return ret;
            }
        }
    }

    match language_id {
        "rust" => traverse_up(path, dir_has_one(&["Cargo.toml"])),
        "php" => traverse_up(path, dir_has_one(&["composer.json"])),
        "javascript" | "typescript" | "javascript.jsx" | "typescript.tsx" => {
            traverse_up(path, dir_has_one(&["package.json"]))
        }
        "python" => traverse_up(
            path,
            dir_has_one(&["setup.py", "Pipfile", "requirements.txt", "pyproject.toml"]),
        ),
        "c" | "cpp" => traverse_up(path, dir_has_one(&["compile_commands.json"])),
        "cs" => traverse_up(path, is_dotnet_root),
        "java" => traverse_up(
            path,
            dir_has_one(&[
                "pom.xml",
                "settings.gradle",
                "settings.gradle.kts",
                "WORKSPACE",
            ]),
        ),
        "scala" => traverse_up(path, dir_has_one(&["build.sbt"])),
        "haskell" => traverse_up(path, dir_has_one(&["stack.yaml"])).or_else(|_| {
            traverse_up(path, |dir| {
                dir_contains_file(dir, |f| has_extension(f, "cabal"))
            })
        }),
        "go" => traverse_up(path, dir_has_one(&["go.mod"])),
        _ => Err(anyhow!("Unknown languageId: {}", language_id)),
    }
    .or_else(|_| {
        traverse_up(path, |dir| {
            dir.join(".git").exists() || dir.join(".hg").exists() || dir.join(".svn").exists()
        })
    })
    .or_else(|_| {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("Failed to get parent dir! path: {:?}", path));
        warn!(
            "Unknown project type. Fallback to use dir as project root: {:?}",
            parent
        );
        parent
    })
}

fn dir_has_one<'a>(files: &'a [&str]) -> impl Fn(&'a Path) -> bool {
    move |dir| files.iter().any(|file| dir.join(file).exists())
}

/// If iterating the directory fails (e.g. because it is not a directory), returns false; if a
/// file cannot be inspected, that file is considered nonexistent.
fn dir_contains_file<F>(path: &Path, predicate: F) -> bool
where
    F: Fn(&Path) -> bool,
{
    if let Ok(diriter) = path.read_dir() {
        for entry in diriter {
            if let Ok(entry) = entry {
                if predicate(&entry.path()) {
                    return true;
                }
            }
        }
    }

    false
}

fn has_extension(path: &Path, ext: &str) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(path_ext) if path_ext == ext => true,
        _ => false,
    }
}

fn traverse_up<'a, F>(path: &'a Path, predicate: F) -> Result<&'a Path>
where
    F: Fn(&'a Path) -> bool,
{
    if predicate(path) {
        return Ok(path);
    }

    let next_path = path.parent().ok_or_else(|| anyhow!("Hit root"))?;

    traverse_up(next_path, predicate)
}

fn is_dotnet_root(dir: &Path) -> bool {
    if dir.join("project.json").exists() {
        return true;
    }
    if !dir.is_dir() {
        return false;
    }

    let entries = match dir.read_dir() {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries {
        if let Ok(entry) = entry {
            if entry.path().ends_with(".csproj") {
                return true;
            }
        }
    }

    false
}

pub trait ToUrl {
    fn to_url(&self) -> Result<Url>;
}

impl<P: AsRef<Path> + std::fmt::Debug> ToUrl for P {
    fn to_url(&self) -> Result<Url> {
        Url::from_file_path(self)
            .or_else(|_| Url::from_str(&self.as_ref().to_string_lossy()))
            .map_err(|_| anyhow!("Failed to convert ({:?}) to Url", self))
    }
}

fn position_to_offset(lines: &[String], position: &Position) -> usize {
    if lines.is_empty() {
        return 0;
    }

    let line = std::cmp::min(position.line as usize, lines.len() - 1);
    let character = std::cmp::min(position.character as usize, lines[line].len());

    let chars_above: usize = lines[..line].iter().map(|text| text.len() + 1).sum();
    chars_above + character
}

fn offset_to_position(lines: &[String], offset: usize) -> Position {
    if lines.is_empty() {
        return Position::new(0, 0);
    }

    let mut offset = offset;
    for (line, text) in lines.iter().enumerate() {
        if offset <= text.len() {
            return Position::new(line as u64, offset as u64);
        }

        offset -= text.len() + 1;
    }

    let last_line = lines.len() - 1;
    let last_character = lines[last_line].len();
    Position::new(last_line as u64, last_character as u64)
}

pub fn apply_text_edits(
    lines: &[String],
    edits: &[TextEdit],
    position: &Position,
) -> Result<(Vec<String>, Position)> {
    // Edits are ordered from bottom to top, from right to left.
    let mut edits_by_index = vec![];
    for edit in edits {
        let start_line = edit.range.start.line.to_usize()?;
        let start_character: usize = edit.range.start.character.to_usize()?;
        let end_line: usize = edit.range.end.line.to_usize()?;
        let end_character: usize = edit.range.end.character.to_usize()?;

        let start = lines[..std::cmp::min(start_line, lines.len())]
            .iter()
            .map(String::len)
            .fold(0, |acc, l| acc + l + 1 /*line ending*/)
            + start_character;
        let end = lines[..std::cmp::min(end_line, lines.len())]
            .iter()
            .map(String::len)
            .fold(0, |acc, l| acc + l + 1 /*line ending*/)
            + end_character;
        edits_by_index.push((start, end, &edit.new_text));
    }

    let mut text = lines.join("\n");
    let mut offset = position_to_offset(&lines, &position);
    for (start, end, new_text) in edits_by_index {
        let start = std::cmp::min(start, text.len());
        let end = std::cmp::min(end, text.len());
        text = String::new() + &text[..start] + new_text + &text[end..];

        // Update offset only if the edit's entire range is before it.
        // Edits after the offset do not affect it.
        // Edits covering the offset cause unpredictable effect.
        if end <= offset {
            offset += new_text.len();
            offset -= new_text.matches("\r\n").count(); // line ending is counted as one offset
            offset -= std::cmp::min(offset, end - start);
        }
    }

    offset = std::cmp::min(offset, text.len());

    let new_lines: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
    let new_position = offset_to_position(&new_lines, offset);
    debug!(
        "Position change after applying text edits: {:?} -> {:?}",
        position, new_position
    );

    Ok((new_lines, new_position))
}

pub trait Combine {
    /// Recursively combine two objects.
    fn combine(&self, other: &Self) -> Self
    where
        Self: Sized + Clone;
}

impl Combine for Value {
    fn combine(&self, other: &Self) -> Self {
        match (self, other) {
            (this, Value::Null) => this.clone(),
            (Value::Object(this), Value::Object(other)) => {
                let mut map = serde_json::Map::new();
                let mut keys: HashSet<String> = HashSet::new();
                for k in this.keys() {
                    keys.insert(k.clone());
                }
                for k in other.keys() {
                    keys.insert(k.clone());
                }
                for k in keys.drain() {
                    let v1 = this.get(&k).unwrap_or(&Value::Null);
                    let v2 = other.get(&k).unwrap_or(&Value::Null);
                    map.insert(k, v1.combine(v2));
                }
                Value::Object(map)
            }
            (_, other) => other.clone(),
        }
    }
}

/// Expand condensed json path as in VSCode.
///
/// e.g.,
/// ```json
/// {
///   "rust.rls": true
/// }
/// ```
/// will be expanded to
/// ```json
/// {
///   "rust": {
///     "rls": true
///   }
/// }
/// ```
pub fn expand_json_path(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut value_expanded = json!({});
            for (k, v) in map {
                let mut v = v;
                for token in k.rsplit('.') {
                    v = json!({ token: v });
                }
                value_expanded = value_expanded.combine(&v);
            }
            value_expanded
        }
        _ => value,
    }
}

pub fn vim_cmd_args_to_value(args: &[String]) -> Result<Value> {
    let mut map = serde_json::map::Map::new();
    for arg in args {
        let mut tokens: Vec<_> = arg.splitn(2, '=').collect();
        tokens.reverse();
        let key = tokens
            .pop()
            .ok_or_else(|| anyhow!("Failed to parse command arguments! tokens: {:?}", tokens))?;
        let value = tokens
            .pop()
            .ok_or_else(|| anyhow!("Failed to parse command arguments! tokens: {:?}", tokens))?;
        let value = Value::String(value.to_owned());
        map.insert(key.to_owned(), value);
    }

    Ok(Value::Object(map))
}

pub fn diff_value<'a>(v1: &'a Value, v2: &'a Value, path: &str) -> HashMap<String, (Value, Value)> {
    let mut diffs = HashMap::new();
    match (v1, v2) {
        (&Value::Null, &Value::Null)
        | (&Value::Bool(_), &Value::Bool(_))
        | (&Value::Number(_), &Value::Number(_))
        | (&Value::String(_), &Value::String(_))
        | (&Value::Array(_), &Value::Array(_)) => {
            if v1 != v2 {
                diffs.insert(path.to_owned(), (v1.clone(), v2.clone()));
            }
        }
        (&Value::Object(ref map1), &Value::Object(ref map2)) => {
            let keys1: HashSet<&String> = map1.keys().collect();
            let keys2: HashSet<&String> = map2.keys().collect();
            for k in keys1.union(&keys2) {
                let mut next_path = String::from(path);
                next_path += ".";
                next_path += k;
                let next_diffs = diff_value(
                    map1.get(*k).unwrap_or(&Value::Null),
                    map2.get(*k).unwrap_or(&Value::Null),
                    &next_path,
                );
                diffs.extend(next_diffs);
            }
        }
        _ => {
            diffs.insert(path.to_owned(), (v1.clone(), v2.clone()));
        }
    }

    diffs
}

pub trait Canonicalize {
    fn canonicalize(&self) -> String;
}

impl<P> Canonicalize for P
where
    P: AsRef<Path>,
{
    fn canonicalize(&self) -> String {
        let path = match std::fs::canonicalize(self) {
            Ok(path) => path.to_string_lossy().into_owned(),
            _ => self.as_ref().to_string_lossy().into_owned(),
        };

        // Trim UNC prefixes.
        // See https://github.com/rust-lang/rust/issues/42869
        path.trim_start_matches("\\\\?\\").into()
    }
}

pub fn get_default_initialization_options(language_id: &str) -> Value {
    match language_id {
        "lsp4j" => json!({
            "extendedClientCapabilities": {
                "classFileContentsSupport": true
            }
        }),
        _ => json!(Value::Null),
    }
}

/// Given a parameter label and its containing signature, return the part before the label, the
/// label itself, and the part after the label.
pub fn decode_parameter_label(
    parameter_label: &lsp_types::ParameterLabel,
    signature: &str,
) -> Result<(String, String, String)> {
    match *parameter_label {
        lsp_types::ParameterLabel::Simple(ref label) => {
            let chunks: Vec<&str> = signature.split(label).collect();
            if chunks.len() != 2 {
                return Err(anyhow!("Parameter is not part of signature"));
            }
            let begin = chunks[0].to_string();
            let label = label.to_string();
            let end = chunks[1].to_string();
            Ok((begin, label, end))
        }
        lsp_types::ParameterLabel::LabelOffsets([start, finish]) => {
            // Offsets are based on a UTF-16 string representation, inclusive start,
            // exclusive finish.
            let start = start.to_usize()?;
            let finish = finish.to_usize()?;
            let utf16: Vec<u16> = signature.encode_utf16().collect();
            let begin = utf16
                .get(..start)
                .ok_or_else(|| anyhow!("Offset out of range"))?;
            let begin = String::from_utf16(begin)?;
            let label = utf16
                .get(start..finish)
                .ok_or_else(|| anyhow!("Offset out of range"))?;
            let label = String::from_utf16(label)?;
            let end = utf16
                .get(finish..)
                .ok_or_else(|| anyhow!("Offset out of range"))?;
            let end = String::from_utf16(end)?;
            Ok((begin, label, end))
        }
    }
}

/// Given a string, convert it into a string for vimscript
/// The string gets surrounded by single quotes.
///
/// Existing single quotes will get escaped by inserting
/// another single quote in place.
///
/// E.g.
/// abcdefg -> 'abcdefg'
/// abdcef'g -> 'abcdef''g'
pub fn convert_to_vim_str(s: &str) -> String {
    let mut vs = String::with_capacity(s.len());

    vs.push('\'');

    for i in s.chars() {
        if i == '\'' {
            vs.push(i);
        }

        vs.push(i);
    }

    vs.push('\'');

    vs
}

/// Converts the kind of a `CodeAction` to a `&str`.
pub fn code_action_kind_as_str(action: &CodeAction) -> &str {
    match action.kind.as_ref().map(|k| k.as_str()) {
        None | Some("") => "action",
        Some(kind) => kind,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lsp_types::Range;

    #[test]
    fn test_escape_single_quote() {
        assert_eq!(escape_single_quote("my' precious"), "my'' precious");
    }

    #[test]
    fn test_position_to_offset() {
        assert_eq!(position_to_offset(&[], &Position::new(0, 0)), 0);

        let lines: Vec<String> = "\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(position_to_offset(&lines, &Position::new(0, 0)), 0);
        assert_eq!(position_to_offset(&lines, &Position::new(0, 1)), 0);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 0)), 0);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 1)), 0);

        let lines: Vec<String> = "a\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(position_to_offset(&lines, &Position::new(0, 0)), 0);
        assert_eq!(position_to_offset(&lines, &Position::new(0, 1)), 1);
        assert_eq!(position_to_offset(&lines, &Position::new(0, 2)), 1);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 0)), 0);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 1)), 1);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 2)), 1);

        let lines: Vec<String> = "a\nbc\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(position_to_offset(&lines, &Position::new(1, 0)), 2);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 1)), 3);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 2)), 4);
        assert_eq!(position_to_offset(&lines, &Position::new(1, 3)), 4);
    }

    #[test]
    fn test_offset_to_position() {
        assert_eq!(offset_to_position(&[], 0), Position::new(0, 0));

        let lines: Vec<String> = "\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(offset_to_position(&lines, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(&lines, 1), Position::new(0, 0));

        let lines: Vec<String> = "a\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(offset_to_position(&lines, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(&lines, 1), Position::new(0, 1));
        assert_eq!(offset_to_position(&lines, 2), Position::new(0, 1));

        let lines: Vec<String> = "a\nbc\n".lines().map(ToOwned::to_owned).collect();
        assert_eq!(offset_to_position(&lines, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(&lines, 1), Position::new(0, 1));
        assert_eq!(offset_to_position(&lines, 2), Position::new(1, 0));
        assert_eq!(offset_to_position(&lines, 3), Position::new(1, 1));
        assert_eq!(offset_to_position(&lines, 4), Position::new(1, 2));
        assert_eq!(offset_to_position(&lines, 5), Position::new(1, 2));
    }
    #[test]
    fn test_apply_text_edit() {
        let lines: Vec<String> = r#"fn main() {
0;
}
"#
        .lines()
        .map(|l| l.to_owned())
        .collect();

        let expect: Vec<String> = r#"fn main() {
    0;
}
"#
        .lines()
        .map(|l| l.to_owned())
        .collect();

        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 3,
                    character: 0,
                },
            },
            new_text: r#"fn main() {
    0;
}
"#
            .to_owned(),
        };

        let position = Position::new(0, 0);

        // Ignore returned position since the edit's range covers current position and the new position
        // is undefined in this case
        let (result, _) = apply_text_edits(&lines, &[edit], &position).unwrap();
        assert_eq!(result, expect);
    }

    #[test]
    fn test_apply_text_edit_overlong_end() {
        use lsp_types::*;

        let lines: Vec<String> = r#"abc = 123"#.lines().map(|l| l.to_owned()).collect();

        let expect: Vec<String> = r#"nb = 123"#.lines().map(|l| l.to_owned()).collect();

        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 99999999,
                    character: 0,
                },
            },
            new_text: r#"nb = 123"#.to_owned(),
        };

        let position = Position::new(0, 1);

        let (result, _) = apply_text_edits(&lines, &[edit], &position).unwrap();
        assert_eq!(result, expect);
    }

    #[test]
    fn test_apply_text_edit_position() {
        use lsp_types::*;

        let lines: Vec<String> = "abc = 123".lines().map(|l| l.to_owned()).collect();

        let expected_lines: Vec<String> = "newline\nabcde = 123"
            .lines()
            .map(|l| l.to_owned())
            .collect();

        let edits = [
            TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 1,
                    },
                    end: Position {
                        line: 0,
                        character: 3,
                    },
                },
                new_text: "bcde".to_owned(),
            },
            TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
                new_text: "newline\n".to_owned(),
            },
        ];

        let position = Position::new(0, 4);
        let expected_position = Position::new(1, 6);

        assert_eq!(
            apply_text_edits(&lines, &edits, &position).unwrap(),
            (expected_lines, expected_position)
        );
    }

    #[test]
    fn test_apply_text_edit_crlf() {
        use lsp_types::*;

        let lines: Vec<String> = "abc = 123".lines().map(|l| l.to_owned()).collect();

        let expected_lines: Vec<String> = "a\r\nbc = 123".lines().map(|l| l.to_owned()).collect();

        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            new_text: "a\r\n".to_owned(),
        };

        let position = Position::new(0, 2);
        let expected_position = Position::new(1, 1);

        assert_eq!(
            apply_text_edits(&lines, &[edit], &position).unwrap(),
            (expected_lines, expected_position)
        );
    }

    #[test]
    fn test_expand_json_path() {
        assert_eq!(
            expand_json_path(json!({
                "k": "v"
            })),
            json!({
                "k": "v"
            })
        );
        assert_eq!(
            expand_json_path(json!({
                "rust.rls": true
            })),
            json!({
                "rust": {
                    "rls": true
                }
            })
        );
        assert_eq!(
            expand_json_path(json!({
                "rust-analyzer.inlayHints.enable": true,
                "rust-analyzer.inlayHints.chainingHints": true,
                "rust-analyzer.rustfmt.overrideCommand": ["rustfmt"],
            })),
            json!({
                "rust-analyzer": {
                    "rustfmt": {
                        "overrideCommand": ["rustfmt"]
                    },
                    "inlayHints": {
                        "enable": true,
                        "chainingHints": true,
                    }
                }
            })
        );
    }

    #[test]
    fn test_vim_cmd_args_to_value() {
        let cmdargs = ["rootPath=/tmp".to_owned()];
        assert_eq!(
            vim_cmd_args_to_value(&cmdargs).unwrap(),
            json!({
                "rootPath": "/tmp"
            })
        );
    }

    #[test]
    fn test_diff_value() {
        use maplit::hashmap;

        assert_eq!(diff_value(&json!({}), &json!({}), "state",), hashmap!());
        assert_eq!(
            diff_value(
                &json!({
                    "line": 1,
                }),
                &json!({
                    "line": 3,
                }),
                "state"
            ),
            hashmap! {
                "state.line".to_owned() => (json!(1), json!(3)),
            }
        );
    }

    #[test]
    fn test_convert_to_vim_str() {
        assert_eq!(convert_to_vim_str("abcdefg"), "'abcdefg'");
        assert_eq!(convert_to_vim_str("'abcdefg"), "'''abcdefg'");
        assert_eq!(convert_to_vim_str("'x'x'x'x'"), "'''x''x''x''x'''");
        assert_eq!(convert_to_vim_str("xyz'''ffff"), "'xyz''''''ffff'");
        assert_eq!(convert_to_vim_str("'''"), "''''''''");
    }
}
