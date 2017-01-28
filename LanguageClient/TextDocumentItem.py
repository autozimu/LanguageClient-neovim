from difflib import SequenceMatcher
from typing import List, Dict, Tuple  # noqa: F401


class TextDocumentItem:
    def __init__(self, uri: str, languageId: str, text: List[str]) -> None:
        self.uri = uri
        self.languageId = languageId
        self.version = 1
        self.text = text

    def incVersion(self) -> int:
        self.version += 1
        return self.version

    def change(self, newText: str) -> Tuple[int, List]:
        changes = []  # type: List[Dict]
        matcher = SequenceMatcher(a=self.text, b=newText)
        for op, i1, i2, j1, j2 in matcher.get_opcodes():
            if op == "replace":
                changes.append({
                    "range": {
                        "start": {"line": i1, "character": 0, },
                        "end": {"line": i1, "character": len(self.text[i1]), },
                        },
                    "rangeLength": len(self.text[i1]),
                    "text": newText[j1],
                    })
        self.text = newText
        return (self.incVersion(), changes)
