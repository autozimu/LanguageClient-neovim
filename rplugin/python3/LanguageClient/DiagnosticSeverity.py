from enum import Enum


class DiagnosticSeverity(Enum):
    Error = 1
    Warning = 2
    Information = 3
    Hint = 4
