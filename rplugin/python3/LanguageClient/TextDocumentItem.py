from typing import List, Dict, Tuple  # noqa: F401
import time


class TextDocumentItem:
    def __init__(self, uri: str, languageId: str, text: str) -> None:
        self.uri = uri
        self.languageId = languageId
        self.version = 1
        self.text = text
        self.last_update = time.time()
        self.dirty = True

    def incVersion(self) -> int:
        self.version += 1
        return self.version

    def change(self, newText: str) -> Tuple[int, List]:
        changes = []  # type: List[Dict]
        changes.append({
            "text": newText
        })
        self.text = newText
        return (self.incVersion(), changes)

    def skip_change(self, threshold: float = 0.5):
        if time.time() - self.last_update < threshold:
            self.dirty = True
            return True
        self.last_update = time.time()
        return False

    def commit_change(self):
        self.dirty = False
