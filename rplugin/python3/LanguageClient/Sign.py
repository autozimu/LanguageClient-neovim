class Sign:
    def __init__(self, line, signname, bufnumber):
        self.line = line   # line number (1 based)
        self.signname = signname
        self.bufnumber = bufnumber

    def __str__(self):
        return '{{"line": {}, "signname": {}, "bufnumber": {}}}'.format(
            self.line, self.signname, self.bufnumber)

    def __hash__(self):
        return self.line ^ hash(self.signname) ^ hash(self.bufnumber)

    def __eq__(self, other):
        return (self.line == other.line and
                self.signname == other.signname and
                self.bufnumber == other.bufnumber)
