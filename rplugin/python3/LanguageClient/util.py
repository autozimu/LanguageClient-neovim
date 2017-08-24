import os
import time
import glob
import difflib
from urllib import parse
from urllib import request
from pathlib import Path
from typing import List, Dict, Callable
from . logger import logger
from . Sign import Sign

currPath = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def joinPath(part):
    return os.path.join(currPath, part)


def getRootPath(filepath: str, languageId: str) -> str:
    rootPath = None
    if languageId == "rust":
        rootPath = traverseUp(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, 'Cargo.toml')))
    elif languageId == "php":
        rootPath = traverseUp(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, "composer.json")))
    elif languageId.startswith("javascript") or languageId == "typescript":
        rootPath = traverseUp(
            filepath, lambda folder:
                os.path.exists(os.path.join(folder, "package.json")))
    elif languageId == "python":
        rootPath = traverseUp(
            filepath, lambda folder: (
                os.path.exists(os.path.join(folder, "__init__.py")) or
                os.path.exists(os.path.join(folder, "setup.py"))))
    elif languageId == "cs":
        rootPath = traverseUp(filepath, isDotnetRoot)
    elif languageId == "java":
        rootPath = traverseUp(filepath, isJavaRoot)
    elif languageId == "haskell":
        rootPath = (traverseUp(
            filepath,
            lambda folder:
                os.path.exists(os.path.join(folder, "stack.yaml"))) or
                    traverseUp(
            filepath,
            lambda folder:
                os.path.exists(os.path.join(folder, ".cabal"))))

    # TODO: detect for other filetypes
    if not rootPath:
        rootPath = traverseUp(
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


def traverseUp(folder: str, predicate: Callable[[str], bool]) -> str:
    if predicate(folder):
        return folder

    next_folder = os.path.dirname(folder)
    if next_folder == folder:  # Prevent infinite loop.
        return None
    else:
        return traverseUp(next_folder, predicate)


def isDotnetRoot(folder: str) -> bool:
    if os.path.exists(os.path.join(folder, "project.json")):
        return True

    if len(glob.glob(os.path.join(folder, "*.csproj"))) > 0:
        return True

    return False


def isJavaRoot(folder: str) -> bool:
    if os.path.exists(os.path.join(folder, ".project")):
        return True

    if os.path.exists(os.path.join(folder, "pom.xml")):
        return True

    return False


def pathToURI(filepath: str) -> str:
    if not os.path.isabs(filepath):
        return None
    return Path(filepath).as_uri()


def uriToPath(uri: str) -> str:
    return request.url2pathname(parse.urlparse(uri).path)


def escape(string: str) -> str:
    return string.replace("'", "''")


def retry(span, count, condition):
    while count > 0 and condition():
        logger.info("retrying...")
        time.sleep(span)
        count -= 1


def getGotoFileCommand(path, bufnames) -> str:
    if path in bufnames:
        return "exe 'buffer ' . fnameescape('{}')".format(path)
    else:
        return "exe 'edit ' . fnameescape('{}')".format(path)


def getCommandDeleteSign(sign: Sign) -> str:
    return " | execute('sign unplace {}')".format(sign.line)


def getCommandAddSign(sign: Sign) -> str:
    return (" | execute('sign place {} line={} "
            "name=LanguageClient{} buffer={}')").format(
                sign.line, sign.line, sign.signname, sign.bufnumber)


def getCommandUpdateSigns(signs: List[Sign], nextSigns: List[Sign]) -> str:
    cmd = "echo"
    diff = difflib.SequenceMatcher(None, signs, nextSigns)
    for op, i1, i2, j1, j2 in diff.get_opcodes():
        if op == "replace":
            for i in range(i1, i2):
                cmd += getCommandDeleteSign(signs[i])
            for i in range(j1, j2):
                cmd += getCommandAddSign(nextSigns[i])
        elif op == "delete":
            for i in range(i1, i2):
                cmd += getCommandDeleteSign(signs[i])
        elif op == "insert":
            for i in range(j1, j2):
                cmd += getCommandAddSign(nextSigns[i])
        elif op == "equal":
            pass
        else:
            msg = "Unknown diff op: " + op
            logger.error(msg)

    return cmd


def convertVimCommandArgsToKwargs(args: List[str]) -> Dict:
    kwargs = {}
    if args:
        for arg in args:
            argarr = arg.split("=")
            if len(argarr) != 2:
                logger.warn("Parse vim command arg failed: " + arg)
                continue
            kwargs[argarr[0]] = argarr[1]
    return kwargs


def apply_TextEdit(textList: List[str], textEdit) -> List[str]:
    startLine = textEdit["range"]["start"]["line"]
    startCharacter = textEdit["range"]["start"]["character"]
    endLine = textEdit["range"]["end"]["line"]
    endCharacter = textEdit["range"]["end"]["character"]
    newText = textEdit["newText"]

    text = "".join(textList)
    startIndex = (sum(map(len, textList[:startLine])) + startLine +
                  startCharacter)
    endIndex = sum(map(len, textList[:endLine])) + endLine + endCharacter
    text = text[:startIndex] + newText + text[endIndex + 1:]
    return text.split("\n")
