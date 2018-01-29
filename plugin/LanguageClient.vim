if exists('g:LanguageClient_loaded') && g:LanguageClient_loaded
    finish
endif

function! s:Echoerr(message) abort
    echohl Error | echomsg a:message | echohl None
endfunction

function! s:Debug(message) abort
    if !exists('g:LanguageClient_loggingLevel')
        return
    endif

    if g:LanguageClient_loggingLevel ==? 'INFO' || g:LanguageClient_loggingLevel ==? 'DEBUG'
        call s:Echoerr(a:message)
    endif
endfunction

let s:id = 1
let s:handlers = {}

" Note: vim execute callback for every line.
let s:content_length = 0
let s:input = ''
function! s:HandleMessage(job, lines, event) abort
    if a:event ==# 'stdout'
        while len(a:lines) > 0
            let l:line = remove(a:lines, 0)

            if l:line ==# ''
                continue
            elseif s:content_length == 0
                let s:content_length = str2nr(substitute(l:line, '.*Content-Length:', '', ''))
                continue
            endif

            let s:input .= strpart(l:line, 0, s:content_length)
            if s:content_length < strlen(l:line)
                call insert(a:lines, strpart(l:line, s:content_length), 0)
                let s:content_length = 0
            else
                let s:content_length = s:content_length - strlen(l:line)
            endif
            if s:content_length > 0
                continue
            endif

            try
                let l:message = json_decode(s:input)
                let s:input = ''
            catch
                let s:input = ''
                call s:Debug(string(v:exception))
                continue
            endtry

            if has_key(l:message, 'method')
                let l:id = get(l:message, 'id', v:null)
                let l:method = get(l:message, 'method')
                let l:params = get(l:message, 'params')
                try
                    if l:method ==# 'execute'
                        for l:cmd in l:params
                            execute l:cmd
                        endfor
                    else
                        let l:result = call(l:method, l:params)
                    endif
                    if l:id != v:null
                        call LanguageClient#Write(json_encode({
                                    \ 'jsonrpc': '2.0',
                                    \ 'id': l:id,
                                    \ 'result': l:result,
                                    \ }))
                    endif
                catch
                    if l:id != v:null
                        call LanguageClient#Write(json_encode({
                                    \ 'jsonrpc': '2.0',
                                    \ 'id': l:id,
                                    \ 'error': {
                                    \   'code': -32603,
                                    \   'message': string(v:exception)
                                    \   }
                                    \ }))
                    endif
                    call s:Debug(string(v:exception))
                endtry
            elseif has_key(l:message, 'result')
                let l:id = get(l:message, 'id')
                let l:result = get(l:message, 'result')
                let Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                if type(Handle) == type(function('tr'))
                    call call(Handle, [l:result, {}])
                elseif type(Handle) == type([])
                    call add(Handle, l:result)
                else
                    call s:Echoerr('Unknown Handle type: ' . string(Handle))
                endif
            elseif has_key(l:message, 'error')
                let l:id = get(l:message, 'id')
                let l:error = get(l:message, 'error')
                let Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                if type(Handle) == type(function('tr'))
                    call s:Echoerr(get(l:error, 'message'))
                    call call(Handle, [{}, l:error])
                elseif type(Handle) == type([])
                    call add(Handle, v:null)
                else
                    call s:Echoerr('Unknown Handle type: ' . string(Handle))
                endif
            else
                call s:Echoerr('Unknown message: ' . string(l:message))
            endif
        endwhile
    elseif a:event ==# 'stderr'
        call s:Echoerr('LanguageClient stderr: ' . string(a:lines))
    elseif a:event ==# 'exit'
        if type(a:lines) == type(0) && a:lines == 0
            return
        endif
        call s:Echoerr('LanguageClient exited with: ' . string(a:lines))
    else
        call s:Debug('LanguageClient unknown event: ' . a:event)
    endif
endfunction

function! s:HandleStdoutVim(job, data) abort
    return s:HandleMessage(a:job, [a:data], 'stdout')
endfunction

function! s:HandleStderrVim(job, data) abort
    return s:HandleMessage(a:job, [a:data], 'stderr')
endfunction

function! s:HandleExitVim(job, data) abort
    return s:HandleMessage(a:job, [a:data], 'exit')
endfunction

let s:root = expand('<sfile>:p:h:h')
function! s:Launch() abort
    if exists('g:LanguageClient_devel')
        if exists('$CARGO_TARGET_DIR')
            let l:command = [$CARGO_TARGET_DIR . '/debug/languageclient']
        else
            let l:command = [s:root . '/target/debug/languageclient']
        endif
    else
        let l:command = [s:root . '/bin/languageclient']
    endif

    if has('nvim')
        let s:job = jobstart(l:command, {
                    \ 'on_stdout': function('s:HandleMessage'),
                    \ 'on_stderr': function('s:HandleMessage'),
                    \ 'on_exit': function('s:HandleMessage'),
                    \ })
        if s:job == 0
            call s:Echoerr('LanguageClient: Invalid arguments!')
            return 0
        elseif s:job == -1
            call s:Echoerr('LanguageClient: Not executable!')
            return 0
        else
            return 1
        endif
    elseif has('job')
        let s:job = job_start(l:command, {
                    \ 'out_cb': function('s:HandleStdoutVim'),
                    \ 'err_cb': function('s:HandleStderrVim'),
                    \ 'exit_cb': function('s:HandleExitVim'),
                    \ })
        if job_status(s:job) !=# 'run'
            call s:Echoerr('LanguageClient: job failed to start or died!')
            return 0
        else
            return 1
        endif
    else
        echoerr 'Not supported: not nvim nor vim with +job.'
        return 0
    endif
endfunction

function! LanguageClient#Write(message) abort
    let l:message = a:message . "\n"
    if has('nvim')
        return jobsend(s:job, l:message)
    elseif has('channel')
        return ch_sendraw(s:job, l:message)
    else
        echoerr 'Not supported: not nvim nor vim with +channel.'
    endif
endfunction

function! LanguageClient#Call(method, params, callback) abort
    let l:id = s:id
    let s:id = s:id + 1
    if a:callback is v:null
        let s:handlers[l:id] = function('HandleOutput')
    else
        let s:handlers[l:id] = a:callback
    endif
    return LanguageClient#Write(json_encode({
                \ 'jsonrpc': '2.0',
                \ 'id': l:id,
                \ 'method': a:method,
                \ 'params': a:params,
                \ }))
endfunction

function! LanguageClient#Notify(method, params) abort
    return LanguageClient#Write(json_encode({
                \ 'jsonrpc': '2.0',
                \ 'method': a:method,
                \ 'params': a:params,
                \ }))
endfunction

function! HandleOutput(result, error) abort
    if len(a:error) > 0
        call s:Echoerr(get(a:error, 'message'))
    else
        let l:result = string(a:result)
        if l:result !=# 'v:null'
            " echomsg l:result
        endif
    endif
endfunction

function! LanguageClient_textDocument_hover(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/hover', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_definition(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'gotoCmd': v:null,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/definition', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_rename(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'cword': expand('<cword>'),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/rename', l:params, v:null)
endfunction

let g:LanguageClient_documentSymbolResults = []
function! LanguageClient_textDocument_documentSymbol(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : g:LanguageClient_documentSymbolResults
    return LanguageClient#Call('textDocument/documentSymbol', l:params, l:callback)
endfunction

let g:LanguageClient_workspaceSymbolResults = []
function! LanguageClient_workspace_symbol(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'query': '',
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : g:LanguageClient_workspaceSymbolResults
    return LanguageClient#Call('workspace/symbol', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_codeAction(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/codeAction', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_completion(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/completion', l:params, l:callback)
endfunction

let g:LanguageClient_referencesResults = []
function! LanguageClient_textDocument_references(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'includeDeclaration': v:true,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : g:LanguageClient_referencesResults
    return LanguageClient#Call('textDocument/references', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_formatting(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/formatting', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_rangeFormatting(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/rangeFormatting', l:params, l:callback)
endfunction

function! LanguageClient_rustDocument_implementations(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('rustDocument/implementations', l:params, l:callback)
endfunction

function! LanguageClient_textDocument_didOpen() abort
    return LanguageClient#Notify('textDocument/didOpen', {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ })
endfunction

function! LanguageClient_textDocument_didChange() abort
    return LanguageClient#Notify('textDocument/didChange', {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'text': getbufline('', 1, '$'),
                \ })
endfunction

function! LanguageClient_textDocument_didSave() abort
    return LanguageClient#Notify('textDocument/didSave', {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ })
endfunction

function! LanguageClient_textDocument_didClose() abort
    return LanguageClient#Notify('textDocument/didClose', {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ })
endfunction

function! LanguageClient_getState(callback) abort
    return LanguageClient#Call('languageClient/getState', {}, a:callback)
endfunction

function! LanguageClient_alive(callback) abort
    return LanguageClient#Call('languageClient/isAlive', {}, a:callback)
endfunction

function! LanguageClient_startServer(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'cmdargs': [],
                \ }
    call extend(l:params, a:0 > 0 ? {'cmdargs': a:000} : {})
    return LanguageClient#Call('languageClient/startServer', l:params, v:null)
endfunction

function! LanguageClient_registerServerCommands(cmds) abort
    return LanguageClient#Call('languageClient/registerServerCommands', a:cmds, v:null)
endfunction

function! LanguageClient_setLoggingLevel(level) abort
    let l:params = {
                \ 'loggingLevel': a:level,
                \ }
    return LanguageClient#Call('languageClient/setLoggingLevel', l:params, v:null)
endfunction

function! LanguageClient_handleBufReadPost() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufReadPost', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient_handleTextChanged() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleTextChanged', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ 'text': getbufline('', 1, '$'),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

let g:LanguageClient_loaded = s:Launch()

function! LanguageClient_handleBufWritePost() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufWritePost', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient_handleBufDelete() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufDelete', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

let s:last_cursor_line = -1
function! LanguageClient_handleCursorMoved() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:cursor_line = line('.')
    if l:cursor_line == s:last_cursor_line
        return
    endif
    let s:last_cursor_line = l:cursor_line

    try
        call LanguageClient#Notify('languageClient/handleCursorMoved', {
                    \ 'buftype': &buftype,
                    \ 'filename': s:Expand('%:p'),
                    \ 'line': line('.') - 1,
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! s:LanguageClient_FZFSinkLocation(line) abort
    return LanguageClient#Notify('LanguageClient_FZFSinkLocation', [a:line])
endfunction

function! s:LanguageClient_FZFSinkCommand(selection) abort
    return LanguageClient#Notify('LanguageClient_FZFSinkCommand', {
                \ 'selection': a:selection,
                \ })
endfunction

function! LanguageClient_NCMRefresh(info, context) abort
    return LanguageClient#Notify('LanguageClient_NCMRefresh', [a:info, a:context])
endfunction

let g:LanguageClient_completeResults = []
function! LanguageClient_omniComplete(...) abort
    try
        let l:params = {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ 'line': line('.') - 1,
                    \ 'character': col('.') - 1,
                    \ }
        call extend(l:params, a:0 >= 1 ? a:1 : {})
        let l:callback = a:0 >= 2 ? a:2 : g:LanguageClient_completeResults
        call LanguageClient#Call('languageClient/omniComplete', l:params, l:callback)
    catch
        call add(g:LanguageClient_completeResults, v:null)
        call s:Debug(string(v:exception))
    endtry
endfunction

function! LanguageClient#complete(findstart, base) abort
    if a:findstart
        let l:line = getline('.')
        let l:start = col('.') - 1
        while l:start > 0 && l:line[l:start - 1] =~# '\w'
            let l:start -= 1
        endwhile
        return l:start
    else
        call LanguageClient_omniComplete({
                    \ 'character': col('.') - 1 + len(a:base),
                    \ })
        while len(g:LanguageClient_completeResults) == 0
            sleep 100m
        endwhile
        return remove(g:LanguageClient_completeResults, 0)
    endif
endfunction

function! LanguageClient_textDocument_signatureHelp(...) abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/signatureHelp', l:params, l:callback)
endfunction

function! LanguageClient_workspace_applyEdit(...) abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:params = {
                \ 'edit': {},
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('workspace/applyEdit', l:params, l:callback)
endfunction

function! LanguageClient_exit() abort
    return LanguageClient#Notify('exit', {
                \ 'languageId': &filetype,
                \ })
endfunction

" Set to 1 when the language server is busy (e.g. building the code).
let g:LanguageClient_serverStatus = 0
let g:LanguageClient_serverStatusMessage = ''

function! LanguageClient_serverStatus() abort
    return g:LanguageClient_serverStatus
endfunction

function! LanguageClient_serverStatusMessage() abort
    return g:LanguageClient_serverStatusMessage
endfunction

" Example function usable for status line.
function! LanguageClient_statusLine() abort
    if 'g:LanguageClient_serverStatusMessage' ==# ''
        return ''
    endif

    return '[' . g:LanguageClient_serverStatusMessage . ']'
endfunction

function! LanguageClient_cquery_base(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('$cquery/base', l:params, l:callback)
endfunction

function! LanguageClient_cquery_derived(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('$cquery/derived', l:params, l:callback)
endfunction

function! LanguageClient_cquery_callers(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('$cquery/callers', l:params, l:callback)
endfunction

function! LanguageClient_cquery_vars(...) abort
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('$cquery/vars', l:params, l:callback)
endfunction

" When editing a [No Name] file, neovim reports filename as "", while vim reports null.
function! s:Expand(exp) abort
    let l:result = expand(a:exp)
    return l:result ==# '' ? '' : l:result
endfunction

" Get all listed buffer file names.
function! s:Bufnames() abort
    return map(filter(range(0,bufnr('$')), 'buflisted(v:val)'), 'fnamemodify(bufname(v:val), '':p'')')
endfunction

function! s:getInput(prompt, default) abort
    call inputsave()
    let l:input = input(a:prompt, a:default)
    call inputrestore()
    return l:input
endfunction

function! s:FZF(source, sink) abort
    let l:options = get(g:, 'LanguageClient_fzfOptions', v:null)
    if l:options == v:null
        let l:options = fzf#vim#with_preview('right:50%:hidden', '?').options
    endif
    call fzf#run(fzf#wrap({
                \ 'source': a:source,
                \ 'sink': function(a:sink),
                \ 'options': l:options,
                \ }))
    if has('nvim')
        call feedkeys('i')
    endif
endfunction

function! s:hasSnippetSupport() abort
    " https://github.com/SirVer/ultisnips
    if exists('did_plugin_ultisnips') || &cp
        return 1
    endif
    " https://github.com/Shougo/neosnippet.vim
    if exists('g:loaded_neosnippet')
        return 1
    endif
    " https://github.com/garbas/vim-snipmate
    if exists('loaded_snips')
        return 1
    endif

    return 0
endfunction

command! -nargs=* LanguageClientStart :call LanguageClient_startServer(<f-args>)
command! LanguageClientStop :call LanguageClient_exit()

augroup languageClient
    autocmd!
    autocmd BufReadPost * call LanguageClient_handleBufReadPost()
    autocmd TextChanged * call LanguageClient_handleTextChanged()
    autocmd TextChangedI * call LanguageClient_handleTextChanged()
    autocmd BufWritePost * call LanguageClient_handleBufWritePost()
    autocmd BufDelete * call LanguageClient_handleBufDelete()
    autocmd CursorMoved * call LanguageClient_handleCursorMoved()
augroup END
