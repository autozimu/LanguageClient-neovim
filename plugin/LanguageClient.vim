if $LANGUAGECLIENT_DEBUG
    if empty($CARGO_TARGET_DIR)
        let s:command = [expand('<sfile>:p:h:h') . '/target/debug/languageclient']
    else
        let s:command = [$CARGO_TARGET_DIR . '/debug/languageclient']
    endif
else
    let s:command = [expand('<sfile>:p:h:h') . '/bin/languageclient']
endif

let s:id = 1
let s:handlers = {}

let s:content_length = 0
let s:input = ''
function! s:HandleMessage(job, lines, event) abort
    if a:event == 'stdout'
        while len(a:lines) > 0
            let l:line = remove(a:lines, 0)

            if l:line ==# ''
                continue
            elseif s:content_length == 0
                let l:line = substitute(l:line, '^Content-Length: ', '', '')
                let s:content_length = str2nr(l:line)
                continue
            endif

            let s:input = s:input . strpart(l:line, 0, s:content_length)
            if s:content_length < len(l:line)
                call insert(a:lines, strpart(l:line, s:content_length), 0)
                let s:content_length = 0
            else
                let s:content_length = s:content_length - len(l:line)
            endif
            if s:content_length != 0
                continue
            endif

            let l:message = json_decode(s:input)
            let s:input = ''
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
                                    \ "jsonrpc": "2.0",
                                    \ "id": l:id,
                                    \ "result": l:result,
                                    \ }))
                    endif
                catch /.*/
                    if l:id != v:null
                        call LanguageClient#Write(json_encode({
                                    \ "jsonrpc": "2.0",
                                    \ "id": l:id,
                                    \ "error": {
                                    \   "code": -32603,
                                    \   "message": string(v:exception)
                                    \   }
                                    \ }))
                    endif
                    if $LANGUAGECLIENT_DEBUG
                        call s:Echoerr(string(v:exception))
                    endif
                endtry
            elseif has_key(l:message, 'result')
                let l:id = get(l:message, 'id')
                let l:result = get(l:message, 'result')
                let Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                if type(Handle) == v:t_func
                    call call(Handle, [l:result, {}])
                elseif type(Handle) == v:t_list
                    call add(Handle, l:result)
                else
                    s:Echoerr('Unknown Handle type: ' . string(Handle))
                endif
            elseif has_key(l:message, 'error')
                let l:id = get(l:message, 'id')
                let l:error = get(l:message, 'error')
                let Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                if type(Handle) == v:t_func
                    call s:Echoerr(get(l:error, 'message'))
                    call call(Handle, [{}, l:error])
                elseif type(Handle) == v:t_list
                    call add(Handle, v:null)
                else
                    s:Echoerr('Unknown Handle type: ' . string(Handle))
                endif
            else
                call s:Echoerr('Unknown message: ' . string(l:message))
            endif
        endwhile
    elseif a:event == 'stderr'
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr('LanguageClient stderr: ' . string(a:lines))
        endif
    elseif a:event == 'exit'
        if a:lines !=# [-1]
            echomsg 'LanguageClient exited with: ' . string(a:lines)
        endif
    else
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr('Unknown event: ' . a:event)
        endif
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

if has('nvim')
    let s:job = jobstart(s:command, {
                \ 'on_stdout': function('s:HandleMessage'),
                \ 'on_stderr': function('s:HandleMessage'),
                \ 'on_exit': function('s:HandleMessage'),
                \ })
elseif has('job')
    let s:job = job_start(s:command, {
                \ 'out_cb': function('s:HandleStdoutVim'),
                \ 'err_cb': function('s:HandleStderrVim'),
                \ 'exit_cb': function('s:HandleExitVim'),
                \ })
else
    echoerr 'Not supported: not nvim nor vim with +job.'
endif

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
        if l:result !=# "v:null"
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
    if &buftype != '' || &filetype == ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufReadPost', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch /.*/
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr("Caught " . string(v:exception))
        endif
    endtry
endfunction

function! LanguageClient_handleTextChanged() abort
    if &buftype != '' || &filetype == ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleTextChanged', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ 'text': getbufline('', 1, '$'),
                    \ })
    catch /.*/
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr("Caught " . string(v:exception))
        endif
    endtry
endfunction

function! LanguageClient_handleBufWritePost() abort
    if &buftype != '' || &filetype == ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufWritePost', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch /.*/
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr("Caught " . string(v:exception))
        endif
    endtry
endfunction

function! LanguageClient_handleBufDelete() abort
    if &buftype != '' || &filetype == ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufDelete', {
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ 'filename': s:Expand('%:p'),
                    \ })
    catch /.*/
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr("Caught " . string(v:exception))
        endif
    endtry
endfunction

let s:last_cursor_line = -1
function! LanguageClient_handleCursorMoved() abort
    if &buftype != '' || &filetype == ''
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
    catch /.*/
        if $LANGUAGECLIENT_DEBUG
            call s:Echoerr("Caught " . string(v:exception))
        endif
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
    let l:params = {
                \ 'buftype': &buftype,
                \ 'languageId': &filetype,
                \ 'filename': s:Expand('%:p'),
                \ 'line': line('.') - 1,
                \ 'character': col('.') - 1,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : g:LanguageClient_completeResults
    call LanguageClient#Call("languageClient/omniComplete", l:params, l:callback)
endfunction

function! LanguageClient#complete(findstart, base) abort
    if a:findstart
        let l:line = getline('.')
        let l:start = col('.') - 1
        while l:start > 0 && l:line[l:start - 1] =~ '\a'
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

function! LanguageClient_exit() abort
    return LanguageClient#Notify('exit', {
                \ 'languageId': &filetype,
                \ })
endfunction

function! s:Echoerr(message) abort
    echohl Error | echomsg a:message | echohl None
endfunction

" When editing a [No Name] file, neovim reports filename as "", while vim reports null.
function! s:Expand(exp) abort
    let l:result = expand(a:exp)
    return l:result ==# '' ? '' : l:result
endfunction

function! s:getInput(prompt, default) abort
    call inputsave()
    let l:input = input(a:prompt, a:default)
    call inputrestore()
    return l:input
endfunction

function! s:FZF(source, sink) abort
    call fzf#run(fzf#wrap({
                \ 'source': a:source,
                \ 'sink': function(a:sink),
                \ }))
    if has('nvim')
        call feedkeys('i')
    endif
endfunction

command! -nargs=* LanguageClientStart :call LanguageClient_startServer(<f-args>)
command! LanguageClientStop :call LanguageClient_exit()

autocmd BufReadPost * call LanguageClient_handleBufReadPost()
autocmd TextChanged * call LanguageClient_handleTextChanged()
autocmd TextChangedI * call LanguageClient_handleTextChanged()
autocmd BufWritePost * call LanguageClient_handleBufWritePost()
autocmd BufDelete * call LanguageClient_handleBufDelete()
autocmd CursorMoved * call LanguageClient_handleCursorMoved()
