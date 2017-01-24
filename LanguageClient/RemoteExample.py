import neovim

@neovim.plugin
class RemoteExample:
    def __init__(self, vim):
        self.vim = vim

    @neovim.command('RemoteExample')
    def RemoteExample(self):
        self.vim.current.line = 'Yo'

