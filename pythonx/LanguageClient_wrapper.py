from LanguageClient import LanguageClient
import vim

lc = LanguageClient(vim)


def getState(*args):
    return lc.getState_vim(args)


def registerServerCommands(*args):
    return lc.registerServerCommands(args)


def alive_vim(*args):
    return lc.alive_nvim(args)


def setLoggingLevel(*args):
    return lc.setLoggingLevel_vim(args)


def start(*args):
    return lc.start(args)


def stop(*args):
    return lc.stop(args)


def initialize(*args):
    return lc.initialize(*args)


def handle_BufReadPost(*args):
    return lc.handle_BufReadPost(args)


def textDocument_didOpen(*args):
    return lc.textDocument_didOpen(args)


def textDocument_didClose(*args):
    return lc.textDocument_didClose(args)


def textDocument_hover(*args):
    return lc.textDocument_hover(args)


def textDocument_definition(*args):
    return lc.textDocument_definition(args)


def textDocument_rename(*args):
    return lc.textDocument_rename(args)


def textDocument_documentSymbol(*args):
    return lc.textDocument_documentSymbol(args)


def workspace_symbol(*args):
    return lc.workspace_symbol(args)


def textDocument_references(*args):
    return lc.textDocument_references(args)


def rustDocument_implementations(*args):
    return lc.rustDocument_implementations(args)


def handle_TextChanged(*args):
    return lc.handle_BufReadPost(args)


def handle_TextChangedI(*args):
    return lc.handle_TextChangedI(args)


def textDocument_didChange(*args):
    return lc.textDocument_didChange(args)


def handle_BufWritePost(*args):
    return lc.handle_BufWritePost(args)


def textDocument_didSave(*args):
    return lc.textDocument_didSave(args)


def textDocument_completion(*args):
    return lc.textDocument_completion(args)


def textDocument_completionOmnifunc(*args):
    return lc.textDocument_completionOmnifunc(args)


def completionManager_refresh(*args):
    return lc.completionManager_refresh(args)


def exit(*args):
    return lc.exit(args)


def handle_CursorMoved(*args):
    return lc.handle_CursorMoved(args)


def completionItem_resolve(*args):
    return lc.completionItem_resolve(args)


def textDocument_signatureHelp(*args):
    return lc.textDocument_signatureHelp(args)


def textDocument_codeAction(*args):
    return lc.textDocument_codeAction(args)


def workspace_executeCommand(*args):
    return lc.workspace_executeCommand(args)


def textDocument_formatting(*args):
    return lc.textDocument_formatting(args)


def textDocument_rangeFormatting(*args):
    return lc.textDocument_rangeFormatting(args)


def call_vim(*args):
    return lc.call_vim(args)


def notify_vim(*args):
    return lc.notify_vim(args)
