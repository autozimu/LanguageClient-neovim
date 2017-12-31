if has('nvim')
    finish
endif

if !exists('*yarp#py3')
    echoerr 'LanguageClient: yarp#py3 does not exist. Refusing to load.'
    finish
endif

command LanguageClientStart call LanguageClient_start()
command LanguageClientStop call LanguageClient_stop()

let s:lc = yarp#py3('LanguageClient_wrapper')

function! LanguageClient_getState()
    return s:lc.call('getState')
endfunction

function! LanguageClient_registerServerCommands(serverCommands)
    return s:lc.call('registerServerCommands', a:serverCommands)
endfunction

function! LanguageClient_alive()
    return s:lc.call('alive_vim')
endfunction

function! LanguageClient_setLoggingLevel(level)
    return s:lc.call('setLoggingLevel', a:level)
endfunction

function! LanguageClient_start()
    return s:lc.call('start')
endfunction

function! LanguageClient_stop()
    return s:lc.call('stop')
endfunction

function! LanguageClient_initialize()
    return s:lc.call('initialize')
endfunction

function! HandleBufReadPost()
    return s:lc.notify('handle_BufReadPost', {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': expand('%:p'),
                \ })
endfunction

autocmd BufReadPost * call HandleBufReadPost()

function! LanguageClient_textDocument_didOpen()
    return s:lc.call('textDocument_didOpen')
endfunction

function! LanguageClient_textDocument_didClose()
    return s:lc.call('textDocument_didClose')
endfunction

function! LanguageClient_workspace_didChangeConfiguration()
    return s:lc.call('workspace_didChangeConfiguration')
endfunction

function! LanguageClient_textDocument_hover()
    return s:lc.call('textDocument_hover')
endfunction

function! LanguageClient_textDocument_definition()
    return s:lc.call('textDocument_definition')
endfunction

function! LanguageClient_textDocument_rename()
    return s:lc.call('textDocument_rename')
endfunction

function! LanguageClient_textDocument_documentSymbol()
    return s:lc.call('textDocument_documentSymbol')
endfunction

function! LanguageClient_workspace_symbol()
    return s:lc.call('workspace_symbol')
endfunction

function! LanguageClient_textDocument_references()
    return s:lc.call('textDocument_references')
endfunction

function! LanguageClient_rustDocument_implementations()
    return s:lc.call('rustDocument_implementations')
endfunction

function! HandleTextChanged()
    return s:lc.notify('handle_TextChanged', {
                \ 'filename': expand('%:p'),
                \ 'buftype': &buftype,
                \ })
endfunction

autocmd TextChanged * call HandleTextChanged()
autocmd TextChangedI * call HandleTextChanged()

function! LanguageClient_textDocument_didChange()
    return s:lc.call('textDocument_didChange')
endfunction

function! HandleBufWritePost()
    return s:lc.notify('handle_BufWritePost', {
                \ 'languageId': &filetype,
                \ 'filename': expand('%:p'),
                \ })
endfunction

autocmd BufWritePost * call HandleBufWritePost()

function! LanguageClient_textDocument_didSave()
    return s:lc.call('textDocument_didSave')
endfunction

function! LanguageClient_textDocument_completion()
    return s:lc.call('textDocument_completion')
endfunction

function! LanguageClient_textDocument_completionOmnifunc()
    return s:lc.call('textDocument_completionOmnifunc')
endfunction

" function! completionManager_refresh()
"     return s:lc.call('completionManager_refresh')
" endfunction

function! LanguageClient_exit()
    return s:lc.call('exit')
endfunction

function! HandleCursorMoved()
    return s:lc.notify('handle_CursorMoved', {
                \ 'buftype': &buftype,
                \ 'line': line('.'),
                \ })
endfunction

autocmd CursorMoved * call HandleCursorMoved()

function! LanguageClient_completionItem_resolve()
    return s:lc.call('completionItem_resolve')
endfunction

function! LanguageClient_textDocument_signatureHelp()
    return s:lc.call('textDocument_signatureHelp')
endfunction

function! LanguageClient_codeAction()
    return s:lc.call('textDocument_codeAction')
endfunction

function! LanguageClient_workspace_executeCommand()
    return s:lc.call('workspace_executeCommand')
endfunction

function! LanguageClient_textDocument_formatting()
    return s:lc.call('textDocument_formatting')
endfunction

function! LanguageClient_textDocument_rangeFormatting()
    return s:lc.call('textDocument_rangeFormatting')
endfunction

function! LanguageClient_call()
    return s:lc.call('call_vim')
endfunction

function! LanguageClient_notify()
    return s:lc.call('notify_vim')
endfunction

function! LanguageClient_completionManager_refresh(...)
    return call(s:lc.notify,['completionManager_refresh'] + a:000, s:lc)
endfunction
