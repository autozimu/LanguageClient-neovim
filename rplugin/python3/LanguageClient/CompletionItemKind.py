CompletionItemKind = {
    1: 'Text',
    2: 'Method',
    3: 'Function',
    4: 'Constructor',
    5: 'Field',
    6: 'Variable',
    7: 'Class',
    8: 'Interface',
    9: 'Module',
    10: 'Property',
    11: 'Unit',
    12: 'Value',
    13: 'Enum',
    14: 'Keyword',
    15: 'Snippet',
    16: 'Color',
    17: 'File',
    18: 'Reference',
}


def convert_CompletionItemKind_to_vim_kind(k: int) -> str:
    if k in [6]:
        return "v"
    elif k in [2, 3, 4]:
        return "f"
    elif k in [5, 10]:
        return "m"
    elif k in [7, 8, 9, 13]:
        return "t"
    else:
        return ""
