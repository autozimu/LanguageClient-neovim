import os
import time
import glob
from urllib import parse
from pathlib import Path
from . logger import logger

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


def traverseUp(folder: str, stop) -> str:
    if stop(folder):
        return folder
    elif folder == "/":
        return None
    else:
        return traverseUp(os.path.dirname(folder), stop)


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
    return parse.urlparse(uri).path


def escape(string: str) -> str:
    return string.replace("'", "''")


def retry(span, count, condition):
    while count > 0 and condition():
        logger.info("retrying...")
        time.sleep(span)
        count -= 1


def getGotoFileCommand(path, bufnames) -> str:
    if path in bufnames:
        return "buffer {}".format(path)
    else:
        return "edit {}".format(path)
