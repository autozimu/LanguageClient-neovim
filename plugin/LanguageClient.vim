if !exists('g:LanguageClient_serverCommands')
    let g:LanguageClient_serverCommands = {}
endif

if !exists('g:LanguageClient_semanticHighlightMaps')
    let g:LanguageClient_semanticHighlightMaps = {}
endif

function! LanguageClient_textDocument_hover(...)
    return call('LanguageClient#textDocument_hover', a:000)
endfunction

function! LanguageClient_textDocument_definition(...)
    return call('LanguageClient#textDocument_definition', a:000)
endfunction

function! LanguageClient_textDocument_typeDefinition(...)
    return call('LanguageClient#textDocument_typeDefinition', a:000)
endfunction

function! LanguageClient_textDocument_implementation(...)
    return call('LanguageClient#textDocument_implementation', a:000)
endfunction

function! LanguageClient_textDocument_rename(...)
    return call('LanguageClient#textDocument_rename', a:000)
endfunction

function! LanguageClient_textDocument_documentSymbol(...)
    return call('LanguageClient#textDocument_documentSymbol', a:000)
endfunction

function! LanguageClient_textDocument_references(...)
    return call('LanguageClient#textDocument_references', a:000)
endfunction

function! LanguageClient_textDocument_codeAction(...)
    return call('LanguageClient#textDocument_codeAction', a:000)
endfunction

function! LanguageClient_textDocument_codeLens(...)
    return call('LanguageClient#textDocument_codeLens', a:000)
endfunction

function! LanguageClient_textDocument_completion(...)
    return call('LanguageClient#textDocument_completion', a:000)
endfunction

function! LanguageClient_textDocument_formatting(...)
    return call('LanguageClient#textDocument_formatting', a:000)
endfunction

function! LanguageClient_textDocument_formatting_sync(...)
    return call('LanguageClient#textDocument_formatting_sync', a:000)
endfunction

function! LanguageClient_textDocument_rangeFormatting(...)
    return call('LanguageClient#textDocument_rangeFormatting', a:000)
endfunction

function! LanguageClient_textDocument_documentHighlight(...)
    return call('LanguageClient#textDocument_documentHighlight', a:000)
endfunction

function! LanguageClient_workspace_symbol(...)
    return call('LanguageClient#workspace_symbol', a:000)
endfunction

function! LanguageClient_workspace_applyEdit(...)
    return call('LanguageClient#workspace_applyEdit', a:000)
endfunction

function! LanguageClient_workspace_executeCommand(...)
    return call('LanguageClient#workspace_executeCommand', a:000)
endfunction

function! LanguageClient_setLoggingLevel(...)
    return call('LanguageClient#setLoggingLevel', a:000)
endfunction

function! LanguageClient_registerServerCommands(...)
    return call('LanguageClient#registerServerCommands', a:000)
endfunction

function! LanguageClient_registerHandlers(...)
    return call('LanguageClient#registerHandlers', a:000)
endfunction

function! LanguageClient_omniComplete(...)
    return call('LanguageClient#omniComplete', a:000)
endfunction

function! LanguageClient_complete(...)
    return call('LanguageClient#complete', a:000)
endfunction

function! LanguageClient_serverStatus(...)
    return call('LanguageClient#serverStatus', a:000)
endfunction

function! LanguageClient_serverStatusMessage(...)
    return call('LanguageClient#serverStatusMessage', a:000)
endfunction

function! LanguageClient_isServerRunning(...)
    return call('LanguageClient#isServerRunning', a:000)
endfunction

function! LanguageClient_statusLine(...)
    return call('LanguageClient#statusLine', a:000)
endfunction

function! LanguageClient_diagnosticsPrevious(...)
    return call('LanguageClient#diagnosticsPrevious', a:000)
endfunction

function! LanguageClient_diagnosticsNext(...)
    return call('LanguageClient#diagnosticsNext', a:000)
endfunction

function! LanguageClient_statusLineDiagnosticsCounts(...)
    return call('LanguageClient#statusLineDiagnosticsCounts', a:000)
endfunction

function! LanguageClient_clearDocumentHighlight(...)
    return call('LanguageClient#clearDocumentHighlight', a:000)
endfunction

function! LanguageClient_cquery_base(...)
    return call('LanguageClient#cquery_base', a:000)
endfunction

function! LanguageClient_cquery_vars(...)
    return call('LanguageClient#cquery_vars', a:000)
endfunction

function! LanguageClient_closeFloatingHover(...)
    return call('LanguageClient#closeFloatingHover', a:000)
endfunction

function! LanguageClient_handleCodeLensAction(...)
    return call('LanguageClient#handleCodeLensAction', a:000)
endfunction

function! LanguageClient_explainErrorAtPoint(...)
    return call('LanguageClient#explainErrorAtPoint', a:000)
endfunction

function! LanguageClient_textDocument_switchSourceHeader(...)
    return call('LanguageClient#textDocument_switchSourceHeader', a:000)
endfunction

function! LanguageClient_showCompletionItemDocumentation(...)
    return call('LanguageClient#showCompletionItemDocumentation', a:000)
endfunction

command! -nargs=* LanguageClientStart :call LanguageClient#startServer(<f-args>)
command! LanguageClientStop call LanguageClient#shutdown()

function! s:OnBufEnter()
  if !LanguageClient#HasCommand(&filetype)
    return
  endif

  call LanguageClient#handleBufEnter()
  call s:ConfigureAutocmds()
endfunction

function! s:OnFileType()
  if !LanguageClient#HasCommand(&filetype)
    return
  endif

  call LanguageClient#handleFileType()
  call s:ConfigureAutocmds()
endfunction

function! s:ConfigureAutocmds()
  augroup languageClient
    autocmd!
    autocmd BufNewFile <buffer> call LanguageClient#handleBufNewFile()
    autocmd BufWritePost <buffer> call LanguageClient#handleBufWritePost()
    autocmd BufDelete <buffer> call LanguageClient#handleBufDelete()
    autocmd TextChanged <buffer> call LanguageClient#handleTextChanged()
    autocmd TextChangedI <buffer> call LanguageClient#handleTextChanged()
    if exists('##TextChangedP')
        autocmd TextChangedP <buffer> call LanguageClient#handleTextChanged()
    endif
    autocmd CursorMoved <buffer> call LanguageClient#handleCursorMoved()
    autocmd VimLeavePre <buffer> call LanguageClient#handleVimLeavePre()
    autocmd CompleteDone <buffer> call LanguageClient#handleCompleteDone()
    if get(g:, 'LanguageClient_signatureHelpOnCompleteDone', 0)
        autocmd CompleteDone <buffer>
                    \ call LanguageClient#textDocument_signatureHelp({}, 's:HandleOutputNothing')
    endif
    if exists('##CompleteChanged') && get(g:, 'LanguageClient_showCompletionDocs', 1)
      autocmd CompleteChanged <buffer> call LanguageClient#handleCompleteChanged(deepcopy(v:event))
    endif

    nnoremap <Plug>(lcn-menu)               :call LanguageClient_contextMenu()<CR>
    nnoremap <Plug>(lcn-hover)              :call LanguageClient_textDocument_hover()<CR>
    nnoremap <Plug>(lcn-rename)             :call LanguageClient_textDocument_rename()<CR>
    nnoremap <Plug>(lcn-definition)         :call LanguageClient_textDocument_definition()<CR>
    nnoremap <Plug>(lcn-type-definition)    :call LanguageClient_textDocument_typeDefinition()<CR>
    nnoremap <Plug>(lcn-references)         :call LanguageClient_textDocument_references()<CR>
    nnoremap <Plug>(lcn-implementation)     :call LanguageClient_textDocument_implementation()<CR>
    nnoremap <Plug>(lcn-code-action)        :call LanguageClient_textDocument_codeAction()<CR>
    vnoremap <Plug>(lcn-code-action)        :call LanguageClient#textDocument_visualCodeAction()<CR>
    nnoremap <Plug>(lcn-code-lens-action)   :call LanguageClient_handleCodeLensAction()<CR>
    nnoremap <Plug>(lcn-symbols)            :call LanguageClient_textDocument_documentSymbol()<CR>
    nnoremap <Plug>(lcn-highlight)          :call LanguageClient_textDocument_documentHighlight()<CR>
    nnoremap <Plug>(lcn-explain-error)      :call LanguageClient_explainErrorAtPoint()<CR>
    nnoremap <Plug>(lcn-format)             :call LanguageClient_textDocument_formatting()<CR>
    nnoremap <Plug>(lcn-format-sync)        :call LanguageClient_textDocument_formatting_sync()<CR>
    nnoremap <Plug>(lcn-diagnostics-next)   :call LanguageClient_diagnosticsNext()<CR>
    nnoremap <Plug>(lcn-diagnostics-prev)   :call LanguageClient_diagnosticsPrevious()<CR>
  augroup END
endfunction

augroup languageClient_fileType
    autocmd!
    autocmd FileType * call s:OnFileType()
    autocmd BufEnter * call s:OnBufEnter()
augroup END
