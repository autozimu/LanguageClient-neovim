from enum import IntEnum
import re


class SymbolKind(IntEnum):
    Unknown = 0
    File = 1
    Module = 2
    Namespace = 3
    Package = 4
    Class = 5
    Method = 6
    Property = 7
    Field = 8
    Constructor = 9
    Enum = 10
    Interface = 11
    Function = 12
    Variable = 13
    Constant = 14
    String = 15
    Number = 16
    Boolean = 17
    Array = 18
    Object = 19
    Key = 20
    Null = 21
    EnumMember = 22
    Struct = 23
    Event = 24
    Operator = 25
    TypeParameter = 26
    # ccls extensions
    # See also https://github.com/Microsoft/language-server-protocol/issues/344
    TypeAlias = 252
    Parameter = 253
    StaticMethod = 254
    Macro = 255

    @staticmethod
    def _missing_(value):
        return SymbolKind.Unknown

    def describe(self):
        return self._pprint_map[self]


SymbolKind._pprint_map = {}
for e in SymbolKind:
    if e == SymbolKind.Unknown:
        s = ""
    else:
        s = re.sub("([a-z])([A-Z])", r"\g<1> \g<2>", str(e).split(".", 1)[1])

    SymbolKind._pprint_map[int(e)] = s
