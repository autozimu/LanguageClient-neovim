if get(g:, 'LanguageClient_loaded')
    finish
endif

function! s:Echo(message) abort
    echo a:message
endfunction

" Echo message without trigger |hit-enter|
function! s:EchoEllipsis(message) abort
    let l:maxlen = &columns * &cmdheight - 2
    if &showcmd
        let maxlen -= 11
    endif
    if &ruler
        let maxlen -= 18
    endif
    if len(a:message) < l:maxlen
        let l:message = a:message
    else
        let l:message = a:message[:l:maxlen - 3] . '...'
    endif
    echo l:message
endfunction

function! s:Echomsg(message) abort
    echomsg a:message
endfunction

function! s:Echoerr(message) abort
    echohl Error | echomsg a:message | echohl None
endfunction

function! s:Echowarn(message) abort
    echohl WarningMsg | echomsg a:message | echohl None
endfunction

function! s:Debug(message) abort
    if !exists('g:LanguageClient_loggingLevel')
        return
    endif

    if g:LanguageClient_loggingLevel ==? 'INFO' || g:LanguageClient_loggingLevel ==? 'DEBUG'
        call s:Echoerr(a:message)
    endif
endfunction

function! s:hasSnippetSupport() abort
    if get(g:, 'LanguageClient_hasSnippetSupport', 1) !=# 1
        return 0
    endif

    " https://github.com/SirVer/ultisnips
    if exists('g:did_plugin_ultisnips')
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

function! s:IsTrue(v) abort
    if type(a:v) ==# type(0)
        return a:v ==# 0 ? v:false : v:true
    elseif a:v is v:null
        return v:false
    else
        return v:true
    endif
endfunction

function! s:IsFalse(v) abort
    return s:IsTrue(a:v) ? v:false : v:true
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
    if !get(g:, 'loaded_fzf')
        call s:Echoerr('FZF not loaded!')
        return
    endif

    if exists('LanguageClient_fzfOptions')
        let l:options = LanguageClient_fzfOptions
    elseif exists('*fzf#vim#with_preview')
        let l:options = fzf#vim#with_preview('right:50%:hidden', '?').options
    else
        let l:options = []
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

function! s:Edit(action, path)
    let l:action = a:action
    " Avoid the not saved warning.
    if l:action ==# 'edit' && bufnr(a:path) != -1
        let l:action = "buffer"
    endif

    execute l:action . ' ' . fnameescape(a:path)
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
                if type(l:message) !=# type({})
                    throw 'Messsage is not dict.'
                endif
            catch
                call s:Debug('Error decoding message: ' . string(v:exception) .
                            \ ' Message: ' . s:input)
                continue
            finally
                let s:input = ''
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
                        let l:result = 0
                    else
                        let l:params = type(l:params) == type([]) ? l:params : [l:params]
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
                    let l:exception = v:exception
                    if l:id != v:null
                        try
                            call LanguageClient#Write(json_encode({
                                        \ 'jsonrpc': '2.0',
                                        \ 'id': l:id,
                                        \ 'error': {
                                        \   'code': -32603,
                                        \   'message': string(v:exception)
                                        \   }
                                        \ }))
                        catch
                            " TODO
                        endtry
                    endif
                    call s:Debug(string(l:exception))
                endtry
            elseif has_key(l:message, 'result') || has_key(l:message, 'error')
                let l:id = get(l:message, 'id')
                " Function name needs to begin with uppercase letter.
                let l:Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                if type(l:Handle) == type(function('tr')) ||
                            \ (type(l:Handle) == type('') && exists('*' . l:Handle))
                    call call(l:Handle, [l:message])
                elseif type(l:Handle) == type([])
                    call add(l:Handle, l:message)
                elseif type(l:Handle) == type('') && exists(l:Handle)
                    let l:outputs = eval(l:Handle)
                    call add(l:outputs, l:message)
                else
                    call s:Echoerr('Unknown Handle type: ' . string(l:Handle))
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

function! s:HandleOutputNothing(output) abort
endfunction

function! s:HandleOutput(output, ...) abort
    let l:quiet = get(a:000, 0)

    if has_key(a:output, 'result')
        " let l:result = string(a:result)
        " if l:result !=# 'v:null'
            " echomsg l:result
        " endif
        return get(a:output, 'result')
    elseif has_key(a:output, 'error')
        let l:error = get(a:output, 'error')
        let l:message = get(l:error, 'message')
        if !l:quiet
            call s:Echoerr(l:message)
        endif
        return v:null
    else
        if !l:quiet
            call s:Echoerr('Unknown output type: ' . json_encode(a:output))
        endif
        return v:null
    endif
endfunction

let s:root = expand('<sfile>:p:h:h')
function! LanguageClient#binaryPath() abort
    let l:filename = 'languageclient'
    if has('win32')
        let l:filename .= '.exe'
    endif

    if exists('g:LanguageClient_devel')
        if exists('$CARGO_TARGET_DIR')
            let l:path = $CARGO_TARGET_DIR . '/debug/'
        else
            let l:path = s:root . '/target/debug/'
        endif
    else
        let l:path = s:root . '/bin/'
    endif

    return l:path . l:filename
endfunction

function! s:Launch() abort
    let l:command = [LanguageClient#binaryPath()]

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
        " jobsend respond 1 for success.
        return !jobsend(s:job, l:message)
    elseif has('channel')
        return ch_sendraw(s:job, l:message)
    else
        echoerr 'Not supported: not nvim nor vim with +channel.'
    endif
endfunction

function! LanguageClient#Call(method, params, callback, ...) abort
    let l:id = s:id
    let s:id = s:id + 1
    if a:callback is v:null
        let s:handlers[l:id] = function('s:HandleOutput')
    else
        let s:handlers[l:id] = a:callback
    endif
    let l:skipAddParams = get(a:000, 0, v:false)
    let l:params = a:params
    if type(a:params) == type({}) && !skipAddParams
        let l:params = extend({
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ }, l:params)
    endif
    return LanguageClient#Write(json_encode({
                \ 'jsonrpc': '2.0',
                \ 'id': l:id,
                \ 'method': a:method,
                \ 'params': l:params,
                \ }))
endfunction

function! LanguageClient#Notify(method, params) abort
    let l:params = a:params
    if type(params) == type({})
        let l:params = extend({
                    \ 'buftype': &buftype,
                    \ 'languageId': &filetype,
                    \ }, l:params)
    endif
    return LanguageClient#Write(json_encode({
                \ 'jsonrpc': '2.0',
                \ 'method': a:method,
                \ 'params': l:params,
                \ }))
endfunction

function! LanguageClient#textDocument_hover(...) abort
    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/hover', l:params, l:callback)
endfunction

" Meta methods to go to various places.
function! LanguageClient#find_locations(method_name, ...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'gotoCmd': v:null,
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call(a:method_name, l:params, l:callback)
endfunction

function! LanguageClient#textDocument_definition(...) abort
    return call('LanguageClient#find_locations', ['textDocument/definition'] + a:000)
endfunction

function! LanguageClient#textDocument_typeDefinition(...) abort
    return call('LanguageClient#find_locations', ['textDocument/typeDefinition'] + a:000)
endfunction

function! LanguageClient#textDocument_implementation(...) abort
    return call('LanguageClient#find_locations', ['textDocument/implementation'] + a:000)
endfunction

function! LanguageClient#textDocument_references(...) abort
    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'includeDeclaration': v:true,
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/references', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_rename(...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'cword': expand('<cword>'),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/rename', l:params, v:null)
endfunction

function! LanguageClient#textDocument_documentSymbol(...) abort
    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/documentSymbol', l:params, l:callback)
endfunction

function! LanguageClient#workspace_symbol(...) abort
    let l:callback = get(a:000, 2, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'query': get(a:000, 0, ''),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 1, {}))
    return LanguageClient#Call('workspace/symbol', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_codeAction(...) abort
    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/codeAction', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_completion(...) abort
    " Note: do not add 'text' as it might be huge.
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': v:false,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/completion', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_formatting(...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/formatting', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_rangeFormatting(...) abort
    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/rangeFormatting', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_rangeFormatting_sync(...) abort
    return !LanguageClient_runSync('LanguageClient#textDocument_rangeFormatting', {
                \ 'handle': v:true,
                \ })
endfunction

function! LanguageClient#rustDocument_implementations(...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('rustDocument/implementations', l:params, l:callback)
endfunction

function! LanguageClient#textDocument_didOpen() abort
    return LanguageClient#Notify('textDocument/didOpen', {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ })
endfunction

function! LanguageClient#textDocument_didChange() abort
    " Note: do not add 'text' as it might be huge.
    return LanguageClient#Notify('textDocument/didChange', {
                \ 'filename': LSP#filename(),
                \ })
endfunction

function! LanguageClient#textDocument_didSave() abort
    return LanguageClient#Notify('textDocument/didSave', {
                \ 'filename': LSP#filename(),
                \ })
endfunction

function! LanguageClient#textDocument_didClose() abort
    return LanguageClient#Notify('textDocument/didClose', {
                \ 'filename': LSP#filename(),
                \ })
endfunction

function! LanguageClient#getState(callback) abort
    return LanguageClient#Call('languageClient/getState', {}, a:callback)
endfunction

function! LanguageClient#alive(callback) abort
    return LanguageClient#Call('languageClient/isAlive', {}, a:callback)
endfunction

function! LanguageClient#startServer(...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'cmdargs': [],
                \ }
    call extend(l:params, a:0 > 0 ? {'cmdargs': a:000} : {})
    return LanguageClient#Call('languageClient/startServer', l:params, v:null)
endfunction

function! LanguageClient#registerServerCommands(cmds, ...) abort
    let l:handle = a:0 > 0 ? a:1 : v:null
    return LanguageClient#Call('languageClient/registerServerCommands', a:cmds, l:handle, v:true)
endfunction

function! LanguageClient#setLoggingLevel(level) abort
    let l:params = {
                \ 'loggingLevel': a:level,
                \ }
    return LanguageClient#Call('languageClient/setLoggingLevel', l:params, v:null)
endfunction

function! LanguageClient#registerHandlers(handlers, ...) abort
    let l:handle = a:0 > 0 ? a:1 : v:null
    return LanguageClient#Call('languageClient/registerHandlers', a:handlers, l:handle)
endfunction

function! s:ExecuteAutocmd(event) abort
    if exists('#User#' . a:event)
        execute 'doautocmd <nomodeline> User ' . a:event
    endif
endfunction

function! LanguageClient_runSync(fn, ...) abort
    let s:LanguageClient_runSync_outputs = []
    let l:arguments = add(a:000[:], s:LanguageClient_runSync_outputs)
    call call(a:fn, l:arguments)
    while len(s:LanguageClient_runSync_outputs) == 0
        sleep 100m
    endwhile
    let l:output = remove(s:LanguageClient_runSync_outputs, 0)
    return s:HandleOutput(l:output, v:true)
endfunction

function! LanguageClient#handleBufReadPost() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufReadPost', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleTextChanged() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        " Note: do not add 'text' as it might be huge.
        call LanguageClient#Notify('languageClient/handleTextChanged', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleBufWritePost() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufWritePost', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleBufDelete() abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufDelete', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

let s:last_cursor_line = -1
function! LanguageClient#handleCursorMoved() abort
    let l:cursor_line = getcurpos()[1] - 1
    if l:cursor_line == s:last_cursor_line
        return
    endif
    let s:last_cursor_line = l:cursor_line

    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleCursorMoved', {
                    \ 'buftype': &buftype,
                    \ 'filename': LSP#filename(),
                    \ 'line': l:cursor_line,
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! s:LanguageClient_FZFSinkLocation(line) abort
    return LanguageClient#Notify('LanguageClient_FZFSinkLocation', [a:line])
endfunction

function! LanguageClient_FZFSinkCommand(selection) abort
    return LanguageClient#Notify('LanguageClient_FZFSinkCommand', {
                \ 'selection': a:selection,
                \ })
endfunction

function! LanguageClient_NCMRefresh(info, context) abort
    return LanguageClient#Call('LanguageClient_NCMRefresh', {
                \ 'info': a:info,
                \ 'ctx': a:context,
                \ }, v:null)
endfunction

function! LanguageClient#explainErrorAtPoint(...) abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'buftype': &buftype,
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('$languageClient/explainErrorAtPoint', l:params, l:callback)
endfunction

let g:LanguageClient_omniCompleteResults = []
function! LanguageClient#omniComplete(...) abort
    try
        " Note: do not add 'text' as it might be huge.
        let l:params = {
                    \ 'filename': LSP#filename(),
                    \ 'line': LSP#line(),
                    \ 'character': LSP#character(),
                    \ 'handle': v:false,
                    \ }
        call extend(l:params, get(a:000, 0, {}))
        let l:callback = get(a:000, 1, g:LanguageClient_omniCompleteResults)
        call LanguageClient#Call('languageClient/omniComplete', l:params, l:callback)
    catch
        call add(l:callback, [])
        call s:Debug(string(v:exception))
    endtry
endfunction

let g:LanguageClient_completeResults = []
function! LanguageClient#complete(findstart, base) abort
    if a:findstart
        let l:line = getline('.')
        let l:cursor = LSP#character()
        let l:input = l:line[:l:cursor]
        let l:start = match(l:input, '\k*$')
        return l:start
    else
        let l:result = LanguageClient_runSync(
                    \ 'LanguageClient#omniComplete', {
                    \ 'character': LSP#character() + len(a:base) })
        return l:result is v:null ? [] : l:result
    endif
endfunction

function! LanguageClient#textDocument_signatureHelp(...) abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/signatureHelp', l:params, l:callback)
endfunction

function! LanguageClient#workspace_applyEdit(...) abort
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

function! LanguageClient#workspace_executeCommand(command, ...) abort
    if &buftype !=# '' || &filetype ==# ''
        return
    endif

    let l:params = {
                \ 'command': a:command,
                \ 'arguments': get(a:000, 0, v:null),
                \ }
    let callback = get(a:000, 1, v:null)
    return LanguageClient#Call('workspace/executeCommand', l:params, l:callback)
endfunction

function! LanguageClient#exit() abort
    return LanguageClient#Notify('exit', {
                \ 'languageId': &filetype,
                \ })
endfunction

" Set to 1 when the language server is busy (e.g. building the code).
let g:LanguageClient_serverStatus = 0
let g:LanguageClient_serverStatusMessage = ''

function! LanguageClient#serverStatus() abort
    return g:LanguageClient_serverStatus
endfunction

function! LanguageClient#serverStatusMessage() abort
    return g:LanguageClient_serverStatusMessage
endfunction

" Example function usable for status line.
function! LanguageClient#statusLine() abort
    if g:LanguageClient_serverStatusMessage ==# ''
        return ''
    endif

    return '[' . g:LanguageClient_serverStatusMessage . ']'
endfunction

function! LanguageClient#cquery_base(...) abort
    return call('LanguageClient#find_locations', ['$cquery/base'] + a:000)
endfunction

function! LanguageClient#cquery_derived(...) abort
    return call('LanguageClient#find_locations', ['$cquery/derived'] + a:000)
endfunction

function! LanguageClient#cquery_callers(...) abort
    return call('LanguageClient#find_locations', ['$cquery/callers'] + a:000)
endfunction

function! LanguageClient#cquery_vars(...) abort
    return call('LanguageClient#find_locations', ['$cquery/vars'] + a:000)
endfunction

function! LanguageClient#java_classFileContent(...) abort
    if &buftype != '' || &filetype ==# ''
        return
    endif

    let l:params = get(a:000, 0, {})
    let l:callback = get(a:000, 1, v:null)
    return LanguageClient#Call('java/classFileContent', l:params, l:callback)
endfunction

function! LanguageClient_contextMenuItems() abort
    return {
                \ 'Code Action': 'LanguageClient#textDocument_codeAction',
                \ 'Definition': 'LanguageClient#textDocument_definition',
                \ 'Document Symbol': 'LanguageClient#textDocument_documentSymbol',
                \ 'Formatting': 'LanguageClient#textDocument_formatting',
                \ 'Hover': 'LanguageClient#textDocument_hover',
                \ 'Implementation': 'LanguageClient#textDocument_implementation',
                \ 'Range Formatting': 'LanguageClient#textDocument_rangeFormatting',
                \ 'References': 'LanguageClient#textDocument_references',
                \ 'Rename': 'LanguageClient#textDocument_rename',
                \ 'Signature Help': 'LanguageClient#textDocument_signatureHelp',
                \ 'Type Definition': 'LanguageClient#textDocument_typeDefinition',
                \ 'Workspace Symbol': 'LanguageClient#workspace_symbol',
                \ }
endfunction

function! LanguageClient_handleContextMenuItem(item) abort
    let l:items = LanguageClient_contextMenuItems()
    silent! exe 'redraw'
    return call(l:items[a:item], [])
endfunction

function! LanguageClient_contextMenu() abort
    let l:options = keys(LanguageClient_contextMenuItems())

    if get(g:, 'loaded_fzf') && get(g:, 'LanguageClient_fzfContextMenu', 1)
        return fzf#run(fzf#wrap({
                    \ 'source': l:options,
                    \ 'sink': function('LanguageClient_handleContextMenuItem'),
                    \ }))
    endif

    let l:selections = map(copy(l:options), { key, val -> printf('%d) %s', key + 1, val ) })

    call inputsave()
    let l:selection = inputlist(l:selections)
    call inputrestore()

    if !l:selection || l:selection > len(l:selections)
        return
    endif

    return LanguageClient_handleContextMenuItem(l:options[l:selection - 1])
endfunction

let g:LanguageClient_loaded = s:Launch()
