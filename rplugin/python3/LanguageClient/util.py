import os
import time
import glob
import difflib
from urllib import parse
from urllib import request
from pathlib import Path
from typing import List, Dict, Callable, Any

import re

from . logger import logger
from . Sign import Sign

project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def join_path(path: str) -> str:
    """
    Join path to this project root (rplugin/python3).
    """
    return os.path.join(project_root, path)


def get_rootPath(filepath: str, languageId: str) -> str:
    rootPath = None
    if languageId == "rust":
        rootPath = traverse_up(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, 'Cargo.toml')))
    elif languageId == "php":
        rootPath = traverse_up(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, "composer.json")))
    elif languageId.startswith("javascript") or languageId == "typescript":
        rootPath = traverse_up(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, "package.json")))
    elif languageId == "python":
        rootPath = traverse_up(
            filepath, lambda folder: (
                os.path.exists(os.path.join(folder, "__init__.py")) or
                os.path.exists(os.path.join(folder, "setup.py"))))
    elif languageId == "cs":
        rootPath = traverse_up(filepath, is_dotnet_root)
    elif languageId == "java":
        rootPath = traverse_up(filepath, is_java_root)
    elif languageId == "haskell":
        rootPath = (traverse_up(filepath,
                                lambda folder: os.path.exists(os.path.join(folder, "stack.yaml"))) or
                    traverse_up(filepath,
                                lambda folder: os.path.exists(os.path.join(folder, ".cabal"))))

    # TODO: detect for other filetypes
    if not rootPath:
        rootPath = traverse_up(
            filepath,
            lambda folder: (
                os.path.exists(os.path.join(folder, ".git")) or
                os.path.exists(os.path.join(folder, ".hg")) or
                os.path.exists(os.path.join(folder, ".svn"))))
    if not rootPath:
        msg = "Unknown project type. Fallback to use dir as project root."
        logger.warn(msg)
        rootPath = os.path.dirname(filepath)
    return rootPath


def traverse_up(folder: str, predicate: Callable[[str], bool]) -> str:
    if predicate(folder):
        return folder

    next_folder = os.path.dirname(folder)
    if next_folder == folder:  # Prevent infinite loop.
        return None
    else:
        return traverse_up(next_folder, predicate)


def is_dotnet_root(folder: str) -> bool:
    if os.path.exists(os.path.join(folder, "project.json")):
        return True

    if len(glob.glob(os.path.join(folder, "*.csproj"))) > 0:
        return True

    return False


def is_java_root(folder: str) -> bool:
    if os.path.exists(os.path.join(folder, ".project")):
        return True

    if os.path.exists(os.path.join(folder, "pom.xml")):
        return True

    return False


def path_to_uri(filepath: str) -> str:
    if not os.path.isabs(filepath):
        return None
    return Path(filepath).as_uri()


def uri_to_path(uri: str) -> str:
    return request.url2pathname(parse.urlparse(uri).path)


def escape(string: str) -> str:
    return string.replace("'", "''")


def retry(span, count, condition):
    while count > 0 and condition():
        logger.info("retrying...")
        time.sleep(span)
        count -= 1


def get_command_goto_file(path, bufnames, l, c) -> str:
    if path in bufnames:
        return "exe 'buffer +:call\\ cursor({},{}) ' . fnameescape('{}')".format(l, c, path)
    else:
        return "exe 'edit +:call\\ cursor({},{}) ' . fnameescape('{}')".format(l, c, path)


def get_command_delete_sign(sign: Sign) -> str:
    return " | execute('sign unplace {}')".format(sign.line)


def get_command_add_sign(sign: Sign) -> str:
    return (" | execute('sign place {} line={} "
            "name=LanguageClient{} buffer={}')").format(
                sign.line, sign.line, sign.signname, sign.bufnumber)


def get_command_update_signs(signs: List[Sign], next_signs: List[Sign]) -> str:
    cmd = "echo"
    diff = difflib.SequenceMatcher(None, signs, next_signs)
    for op, i1, i2, j1, j2 in diff.get_opcodes():
        if op == "replace":
            for i in range(i1, i2):
                cmd += get_command_delete_sign(signs[i])
            for i in range(j1, j2):
                cmd += get_command_add_sign(next_signs[i])
        elif op == "delete":
            for i in range(i1, i2):
                cmd += get_command_delete_sign(signs[i])
        elif op == "insert":
            for i in range(j1, j2):
                cmd += get_command_add_sign(next_signs[i])
        elif op == "equal":
            pass
        else:
            msg = "Unknown diff op: " + op
            logger.error(msg)

    return cmd


def convert_vim_command_args_to_kwargs(args: List[str]) -> Dict:
    kwargs = {}
    if args:
        for arg in args:
            arr = arg.split("=")
            if len(arr) != 2:
                logger.warn("Parse vim command arg failed: " + arg)
                continue
            kwargs[arr[0]] = arr[1]
    return kwargs


def apply_TextEdit(text_list: List[str], textEdit: Dict) -> List[str]:
    start_line = textEdit["range"]["start"]["line"]
    start_character = textEdit["range"]["start"]["character"]
    end_line = textEdit["range"]["end"]["line"]
    end_character = textEdit["range"]["end"]["character"]
    newText = textEdit["newText"]

    text = str.join("\n", text_list)
    start_index = (sum(map(len, text_list[:start_line])) + start_line + start_character)
    end_index = sum(map(len, text_list[:end_line])) + end_line + end_character
    text = text[:start_index] + newText + text[end_index:]
    return text.split("\n")


def markedString_to_str(s: Any) -> str:
    if isinstance(s, str):
        # Roughly convert markdown to plain text.
        return re.sub(r'\\([\\`*_{}[\]()#+\-.!])', r'\1', s)
    else:
        return s["value"]


def convert_lsp_completion_item_to_vim_style(item):
    insertText = item.get('insertText', "") or ""
    label = item['label']

    e = {}
    e['icase'] = 1
    # insertText:
    # A string that should be inserted a document when selecting
    # this completion. When `falsy` the label is used.
    e['word'] = insertText or label
    e['abbr'] = label
    e['dup'] = 1
    e['menu'] = item.get('detail', "")
    e['info'] = item.get('documentation', "")

    return e
