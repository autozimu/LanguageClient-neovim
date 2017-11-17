from .DiagnosticSeverity import DiagnosticSeverity


class Sign:
    def __init__(self, line: int, severity: DiagnosticSeverity) -> None:
        self.id = 75000 + (line - 1) * 4 + severity.value - 1
        self.line = line
        self.severity = severity

    def __eq__(self, other):
        if isinstance(self, other.__class__):
            return self.id == other.id
        return False

    def __lt__(self, other):
        return self.id < other.id

    def __hash__(self):
        return self.id

    def __repr__(self):
        return "({}, {})".format(self.line, self.severity)
