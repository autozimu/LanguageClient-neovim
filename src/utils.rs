use std;
use types::*;
use serde_json;

pub fn escape_single_quote(s: &str) -> String {
    s.replace("'", "''")
}

#[test]
fn test_escape_single_quote() {
    assert_eq!(escape_single_quote("my' precious"), "my'' precious");
}

pub fn get_rootPath<'a>(path: &'a Path, languageId: &str, rootMarkers: &Option<RootMarkers>) -> Result<&'a Path> {
    if let Some(ref rootMarkers) = *rootMarkers {
        let empty = vec![];
        let rootMarkers = match *rootMarkers {
            RootMarkers::Array(ref arr) => arr,
            RootMarkers::Map(ref map) => map.get(languageId).unwrap_or(&empty),
        };

        for marker in rootMarkers {
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

    match languageId {
        "rust" => traverse_up(path, |dir| dir.join("Cargo.toml").exists()),
        "php" => traverse_up(path, |dir| dir.join("composer.json").exists()),
        "javascript" | "typescript" => traverse_up(path, |dir| dir.join("package.json").exists()),
        "python" => traverse_up(path, |dir| {
            dir.join("__init__.py").exists() || dir.join("setup.py").exists()
        }),
        "c" | "cpp" => traverse_up(path, |dir| dir.join("compile_commands.json").exists()),
        "cs" => traverse_up(path, is_dotnet_root),
        "java" => traverse_up(path, |dir| {
            dir.join(".project").exists() || dir.join("pom.xml").exists()
        }),
        "haskell" => traverse_up(path, |dir| dir.join("stack.yaml").exists())
            .or_else(|_| traverse_up(path, |dir| dir.join(".cabal").exists())),
        _ => Err(format_err!("Unknown languageId: {}", languageId)),
    }.or_else(|_| {
        traverse_up(path, |dir| {
            dir.join(".git").exists() || dir.join(".hg").exists() || dir.join(".svn").exists()
        })
    })
        .or_else(|_| {
            let parent = path.parent()
                .ok_or_else(|| format_err!("Failed to get file dir"));
            warn!(
                "Unknown project type. Fallback to use dir as project root: {:?}",
                parent
            );
            parent
        })
}

fn traverse_up<F>(path: &Path, predicate: F) -> Result<&Path>
where
    F: Fn(&Path) -> bool,
{
    if predicate(path) {
        return Ok(path);
    }

    let next_path = path.parent().ok_or_else(|| format_err!("Hit root"))?;

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
        Url::from_file_path(self).or_else(|_| {
            Err(format_err!(
                "Failed to convert from path ({:?}) to Url",
                self
            ))
        })
    }
}

pub fn get_logpath() -> PathBuf {
    let dir = env::var("TMP")
        .or_else(|_| env::var("TEMP"))
        .unwrap_or_else(|_| "/tmp".to_owned());

    Path::new(&dir).join("LanguageClient.log")
}

pub fn get_logpath_server() -> PathBuf {
    let dir = env::var("TMP")
        .or_else(|_| env::var("TEMP"))
        .unwrap_or_else(|_| "/tmp".to_owned());

    Path::new(&dir).join("LanguageServer.log")
}

pub fn get_log_server() -> Result<String> {
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .open(get_logpath_server())?;
    let mut buffer = String::new();
    f.read_to_string(&mut buffer)?;
    Ok(buffer)
}

pub trait Strip {
    fn strip(&self) -> Self;
}

impl<'a> Strip for &'a str {
    fn strip(&self) -> Self {
        self.trim().trim_matches(|c| c == '\r' || c == '\n')
    }
}

impl Strip for String {
    fn strip(&self) -> Self {
        self.as_str().strip().to_owned()
    }
}

pub fn apply_TextEdits(lines: &[String], edits: &[TextEdit]) -> Result<Vec<String>> {
    // Edits are ordered from bottom to top, from right to left.
    let mut edits_by_index = vec![];
    for edit in edits {
        let start_line = edit.range.start.line.to_usize()?;
        let start_character: usize = edit.range.start.character.to_usize()?;
        let end_line: usize = edit.range.end.line.to_usize()?;
        let end_character: usize = edit.range.end.character.to_usize()?;

        let start = lines[..start_line]
            .iter()
            .map(|l| l.len())
            .fold(0, |acc, l| acc + l + 1 /*line ending*/) + start_character;
        let end = lines[..end_line]
            .iter()
            .map(|l| l.len())
            .fold(0, |acc, l| acc + l + 1 /*line ending*/) + end_character;

        edits_by_index.push((start, end, &edit.new_text));
    }

    let mut text = lines.join("\n");
    for (start, end, new_text) in edits_by_index {
        text = String::new() + &text[..start] + new_text + &text[end..];
    }

    Ok(text.split('\n').map(|l| l.to_owned()).collect())
}

#[test]
fn test_apply_TextEdit() {
    let lines: Vec<String> = r#"fn main() {
0;
}
"#.split('\n')
        .map(|l| l.to_owned())
        .collect();

    let expect: Vec<String> = r#"fn main() {
    0;
}
"#.split('\n')
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
"#.to_owned(),
    };

    assert_eq!(apply_TextEdits(&lines, &[edit]).unwrap(), expect);
}

fn get_command_add_sign(sign: &Sign, filename: &str) -> String {
    format!(
        " | execute 'sign place {} line={} name=LanguageClient{:?} file={}'",
        sign.id, sign.line, sign.severity, filename
    )
}

#[test]
fn test_get_command_add_sign() {
    let sign = Sign::new(1, DiagnosticSeverity::Error);
    assert_eq!(
        get_command_add_sign(&sign, ""),
        " | execute 'sign place 75000 line=1 name=LanguageClientError file='"
    );

    let sign = Sign::new(7, DiagnosticSeverity::Error);
    assert_eq!(
        get_command_add_sign(&sign, ""),
        " | execute 'sign place 75024 line=7 name=LanguageClientError file='"
    );

    let sign = Sign::new(7, DiagnosticSeverity::Hint);
    assert_eq!(
        get_command_add_sign(&sign, ""),
        " | execute 'sign place 75027 line=7 name=LanguageClientHint file='"
    );
}

fn get_command_delete_sign(sign: &Sign, filename: &str) -> String {
    format!(" | execute 'sign unplace {} file={}'", sign.id, filename)
}

#[test]
fn test_get_command_delete_sign() {}

use diff;

pub fn get_command_update_signs(signs_prev: &[Sign], signs: &[Sign], filename: &str) -> String {
    let mut cmd = "echo".to_owned();
    for comp in diff::slice(signs_prev, signs) {
        match comp {
            diff::Result::Left(sign) => {
                cmd += &get_command_delete_sign(sign, filename);
            }
            diff::Result::Right(sign) => {
                cmd += &get_command_add_sign(sign, filename);
            }
            diff::Result::Both(_, _) => (),
        }
    }

    cmd
}

#[test]
fn test_get_command_update_signs() {}

pub trait Merge {
    fn merge(&mut self, other: Self) -> ();
}

impl<K, V> Merge for HashMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    fn merge(&mut self, other: Self) -> () {
        for (k, v) in other {
            self.insert(k, v);
        }
    }
}

pub trait Combine {
    fn combine(self, other: Self) -> Self
    where
        Self: Sized;
}

impl Combine for Value {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (this, Value::Null) => this,
            (Value::Object(this), Value::Object(other)) => {
                let mut map = serde_json::map::Map::new();
                let mut keys: HashSet<String> = HashSet::new();
                for k in this.keys() {
                    keys.insert(k.clone());
                }
                for k in other.keys() {
                    keys.insert(k.clone());
                }
                for k in keys.drain() {
                    let v1 = this.get(&k).unwrap_or(&Value::Null).clone();
                    let v2 = other.get(&k).unwrap_or(&Value::Null).clone();
                    map.insert(k, v1.combine(v2));
                }
                Value::Object(map)
            }
            (_, other) => other,
        }
    }
}

pub fn vim_cmd_args_to_value(args: &[String]) -> Result<Value> {
    let mut map = serde_json::map::Map::new();
    for arg in args {
        let mut tokens: Vec<_> = arg.splitn(2, '=').collect();
        tokens.reverse();
        let key = tokens
            .pop()
            .ok_or_else(|| format_err!("Failed to parse command arguments."))?;
        let value = tokens
            .pop()
            .ok_or_else(|| format_err!("Failed to parse command arguments."))?;
        let value = Value::String(value.to_owned());
        map.insert(key.to_owned(), value);
    }

    Ok(Value::Object(map))
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

pub fn diff_value<'a>(v1: &'a Value, v2: &'a Value, path: &str) -> HashMap<String, (Value, Value)> {
    let mut diffs = HashMap::new();
    match (v1, v2) {
        (&Value::Null, &Value::Null)
        | (&Value::Bool(_), &Value::Bool(_))
        | (&Value::Number(_), &Value::Number(_))
        | (&Value::String(_), &Value::String(_))
        | (&Value::Array(_), &Value::Array(_)) => if v1 != v2 {
            diffs.insert(path.to_owned(), (v1.clone(), v2.clone()));
        },
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
                diffs.merge(next_diffs);
            }
        }
        _ => {
            diffs.insert(path.to_owned(), (v1.clone(), v2.clone()));
        }
    }

    diffs
}

#[test]
fn test_diff_value() {
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
        hashmap!{
            "state.line".to_owned() => (json!(1), json!(3)),
        }
    );
}

pub trait Canonicalize {
    fn canonicalize(&self) -> String;
}

impl<P> Canonicalize for P
where
    P: AsRef<Path>,
{
    fn canonicalize(&self) -> String {
        if let Ok(fc) = std::fs::canonicalize(self) {
            if let Some(fs) = fc.to_str() {
                return fs.to_owned();
            }
        }

        self.as_ref()
            .to_str()
            .map(|s| s.to_owned())
            .unwrap_or_default()
    }
}
