from typing import List, Dict, Tuple  # noqa: F401


class TextDocumentItem:
    def __init__(self, uri: str, languageId: str, text: str) -> None:
        self.uri = uri
        self.languageId = languageId
        self.version = 1
        self.text = text

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
