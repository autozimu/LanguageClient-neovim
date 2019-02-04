if get(g:, 'LanguageClient_loaded')
    finish
endif

let s:TYPE = {
\   'string':  type(''),
\   'list':    type([]),
\   'dict':    type({}),
\   'funcref': type(function('call'))
\ }

function! s:AddPrefix(message) abort
    return '[LC] ' . a:message
endfunction

function! s:Echo(message) abort
    echo a:message
endfunction

function! s:Ellipsis(message) abort
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
    return l:message
endfunction

" `echo` message without trigger |hit-enter|
function! s:EchoEllipsis(message) abort
    echo s:Ellipsis(a:message)
endfunction

" `echomsg` message without trigger |hit-enter|
function! s:EchomsgEllipsis(message) abort
    " Credit: ALE, snippets from ale#cursor#TruncatedEcho()
    let l:message = s:AddPrefix(a:message)
    " Change tabs to spaces.
    let l:message = substitute(l:message, "\t", ' ', 'g')
    " Remove any newlines in the message.
    let l:message = substitute(l:message, "\n", '', 'g')

    " We need to remember the setting for shortmess and reset it again.
    let l:shortmess_options = &l:shortmess
    try
        let l:cursor_position = getcurpos()

        " The message is truncated and saved to the history.
        setlocal shortmess+=T
        exec "norm! :echomsg l:message\n"

        " Reset the cursor position if we moved off the end of the line.
        " Using :norm and :echomsg can move the cursor off the end of the
        " line.
        if l:cursor_position != getcurpos()
            call setpos('.', l:cursor_position)
        endif
    finally
        let &l:shortmess = l:shortmess_options
    endtry
endfunction

function! s:Echomsg(message) abort
    echomsg s:AddPrefix(a:message)
endfunction

function! s:Echoerr(message) abort
    echohl Error | echomsg s:AddPrefix(a:message) | echohl None
endfunction

function! s:Echowarn(message) abort
    echohl WarningMsg | echomsg s:AddPrefix(a:message) | echohl None
endfunction

" timeout: skip function call f until this timeout, in seconds.
function! s:Debounce(timeout, f) abort
    " Map function to its last execute time.
    let s:DebounceMap = {}
    let l:lastexectime = get(s:DebounceMap, a:f)
    if l:lastexectime == 0 || reltimefloat(reltime(l:lastexectime)) < a:timeout
        let s:DebounceMap[a:f] = reltime()
        return v:true
    else
        return v:false
    endif
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
    if exists('g:LanguageClient_hasSnippetSupport')
        return g:LanguageClient_hasSnippetSupport !=# 0
    endif

    " https://github.com/Shougo/neosnippet.vim
    if exists('g:loaded_neosnippet')
        return 1
    endif

    return 0
endfunction

function! s:useVirtualText() abort
    let l:use = s:GetVar('LanguageClient_useVirtualText')
    if l:use isnot v:null
        return !!l:use
    endif

    return exists('*nvim_buf_set_virtual_text')
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

" Clear and set virtual texts between line_start and line_end (exclusive).
function! s:set_virtual_texts(buf_id, ns_id, line_start, line_end, virtual_texts) abort
    " VirtualText: map with keys line, text and hl_group.

    if !exists('*nvim_buf_set_virtual_text')
        return
    endif

    call nvim_buf_clear_namespace(a:buf_id, a:ns_id, a:line_start, a:line_end)

    for vt in a:virtual_texts
        call nvim_buf_set_virtual_text(a:buf_id, a:ns_id, vt['line'], [[vt['text'], vt['hl_group']]], {})
    endfor
endfunction

" Execute serious of ex commands.
function! s:command(...) abort
    for l:cmd in a:000
        execute l:cmd
    endfor
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

    let l:options = s:GetVar('LanguageClient_fzfOptions')
    if l:options is v:null
        if exists('*fzf#vim#with_preview')
            let l:options = fzf#vim#with_preview('right:50%:hidden', '?').options
        else
            let l:options = []
        endif
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

function! s:Edit(action, path) abort
    " If editing current file, push current location to jump list.
    let l:bufnr = bufnr(a:path)
    if l:bufnr == bufnr('%')
        execute 'normal m`'
        return
    endif

    let l:action = a:action
    " Avoid the 'not saved' warning.
    if l:action ==# 'edit' && l:bufnr != -1
        execute 'buffer' l:bufnr
        set buflisted
        return
    endif

    execute l:action . ' ' . fnameescape(a:path)
endfunction

" Batch version of `matchdelete()`.
function! s:MatchDelete(ids) abort
    for l:id in a:ids
        call matchdelete(l:id)
    endfor
endfunction

" Batch version of nvim_buf_add_highlight
function! s:AddHighlights(source, highlights) abort
    for hl in a:highlights
        call nvim_buf_add_highlight(0, a:source, hl.group, hl.line, hl.character_start, hl.character_end)
    endfor
endfunction

" Get an variable value.
" Get variable from uffer local, or else global, or else default, or else v:null.
function! s:GetVar(...) abort
    let name = a:1

    if exists('b:' . name)
        return get(b:, name)
    elseif exists('g:' . name)
        return get(g:, name)
    else
        return get(a:000, 1, v:null)
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
                if type(l:message) !=# s:TYPE.dict
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
                    let l:params = type(l:params) == s:TYPE.list ? l:params : [l:params]
                    let l:result = call(l:method, l:params)
                    if l:id isnot v:null
                        call LanguageClient#Write(json_encode({
                                    \ 'jsonrpc': '2.0',
                                    \ 'id': l:id,
                                    \ 'result': l:result,
                                    \ }))
                    endif
                catch
                    let l:exception = v:exception
                    if l:id isnot v:null
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
                let l:Handle = get(s:handlers, l:id)
                unlet s:handlers[l:id]
                let l:type = type(l:Handle)
                if l:type == s:TYPE.funcref || l:type == s:TYPE.string
                    call call(l:Handle, [l:message])
                elseif l:type == s:TYPE.list
                    call add(l:Handle, l:message)
                elseif l:type == s:TYPE.string && exists(l:Handle)
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
        call s:Debug('LanguageClient exited with: ' . string(a:lines))
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
        " if l:result isnot v:null
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
    let l:binpath = LanguageClient#binaryPath()

    if executable(l:binpath) != 1
        call s:Echoerr('LanguageClient: binary (' . l:binpath . ') doesn''t exists! Please check installation guide.')
        return 0
    endif

    if has('nvim')
        let s:job = jobstart([l:binpath], {
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
        let s:job = job_start([l:binpath], {
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
    if &buftype !=# '' || &filetype ==# '' || expand('%') ==# ''
        " call s:Debug('Skip sending message')
        return
    endif

    let l:id = s:id
    let s:id = s:id + 1
    if a:callback is v:null
        let s:handlers[l:id] = function('s:HandleOutput')
    else
        let s:handlers[l:id] = a:callback
    endif
    let l:skipAddParams = get(a:000, 0, v:false)
    let l:params = a:params
    if type(a:params) == s:TYPE.dict && !skipAddParams
        " TODO: put inside context.
        let l:params = extend({
                    \ 'bufnr': bufnr(''),
                    \ 'languageId': &filetype,
                    \ 'viewport': LSP#viewport(),
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
    if &buftype !=# '' || &filetype ==# '' || expand('%') ==# ''
        " call s:Debug('Skip sending message')
        return
    endif

    let l:params = a:params
    if type(params) == s:TYPE.dict
        let l:params = extend({
                    \ 'bufnr': bufnr(''),
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
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/hover', l:params, l:Callback)
endfunction

" Meta methods to go to various places.
function! LanguageClient#findLocations(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'gotoCmd': v:null,
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('languageClient/findLocations', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_definition(...) abort
    let l:params = {
                \ 'method': 'textDocument/definition',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#textDocument_typeDefinition(...) abort
    let l:params = {
                \ 'method': 'textDocument/typeDefinition',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#textDocument_implementation(...) abort
    let l:params = {
                \ 'method': 'textDocument/implementation',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#textDocument_references(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'includeDeclaration': v:true,
                \ 'handle': s:IsFalse(l:Callback),
                \ 'gotoCmd': v:null,
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/references', l:params, l:Callback)
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
    let l:Callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/rename', l:params, v:null)
endfunction

function! LanguageClient#textDocument_documentSymbol(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/documentSymbol', l:params, l:Callback)
endfunction

function! LanguageClient#workspace_symbol(...) abort
    let l:Callback = get(a:000, 2, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'query': get(a:000, 0, ''),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 1, {}))
    return LanguageClient#Call('workspace/symbol', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_codeAction(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/codeAction', l:params, l:Callback)
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
    let l:Callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/completion', l:params, l:Callback)
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
    let l:Callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/formatting', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_formatting_sync(...) abort
    let l:result = LanguageClient_runSync('LanguageClient#textDocument_formatting', {
                \ 'handle': v:true,
                \ })
    return l:result isnot v:null
endfunction

function! LanguageClient#textDocument_rangeFormatting(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'LSP#range_start_line()': LSP#range_start_line(),
                \ 'LSP#range_end_line()': LSP#range_end_line(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/rangeFormatting', l:params, l:Callback)
endfunction

function! LanguageClient#completionItem_resolve(completion_item, ...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'completionItem': a:completion_item,
                \ 'handle': s:IsFalse(l:Callback)
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('completionItem/resolve', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_rangeFormatting_sync(...) abort
    let l:result = LanguageClient_runSync('LanguageClient#textDocument_rangeFormatting', {
                \ 'handle': v:true,
                \ })
    return l:result isnot v:null
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

function! LanguageClient#textDocument_documentHighlight(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/documentHighlight', l:params, l:Callback)
endfunction

function! LanguageClient#clearDocumentHighlight() abort
    return LanguageClient#Notify('languageClient/clearDocumentHighlight', {})
endfunction

function! LanguageClient#getState(callback) abort
    return LanguageClient#Call('languageClient/getState', {}, a:callback)
endfunction

function! LanguageClient#isAlive(callback) abort
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

function! LanguageClient#setDiagnosticsList(diagnosticsList) abort
    let l:params = {
                \ 'diagnosticsList': a:diagnosticsList,
                \ }
    return LanguageClient#Call('languageClient/setDiagnosticsList', l:params, v:null)
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
    let l:LanguageClient_runSync_outputs = []
    let l:arguments = add(a:000[:], l:LanguageClient_runSync_outputs)
    call call(a:fn, l:arguments)
    while len(l:LanguageClient_runSync_outputs) == 0
        sleep 100m
    endwhile
    let l:output = remove(l:LanguageClient_runSync_outputs, 0)
    return s:HandleOutput(l:output, v:true)
endfunction

function! LanguageClient#handleBufNewFile() abort
    try
        call LanguageClient#Notify('languageClient/handleBufNewFile', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleFileType() abort
    try
        if s:Debounce(2, 'LanguageClient#handleFileType')
            call LanguageClient#Notify('languageClient/handleFileType', {
                        \ 'filename': LSP#filename(),
                        \ })
        endif
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleTextChanged() abort
    if &buftype !=# '' || &filetype ==# '' || expand('%') ==# ''
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
    try
        call LanguageClient#Notify('languageClient/handleBufWritePost', {
                    \ 'filename': LSP#filename(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleBufDelete() abort
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

    try
        call LanguageClient#Notify('languageClient/handleCursorMoved', {
                    \ 'buftype': &buftype,
                    \ 'filename': LSP#filename(),
                    \ 'line': l:cursor_line,
                    \ 'LSP#visible_line_start()': LSP#visible_line_start(),
                    \ 'LSP#visible_line_end()': LSP#visible_line_end(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleCompleteDone() abort
    let user_data = get(v:completed_item, 'user_data', '')
    if user_data ==# ''
        return
    endif

    try
        call LanguageClient#Notify('languageClient/handleCompleteDone', {
                    \ 'filename': LSP#filename(),
                    \ 'completed_item': v:completed_item,
                    \ 'line': LSP#line(),
                    \ 'character': LSP#character(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
endfunction

function! LanguageClient#handleVimLeavePre() abort
    try
        if get(g:, 'LanguageClient_autoStop', 1)
            call LanguageClient#exit()
        endif
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

function! LanguageClient_NCM2OnComplete(context) abort
    return LanguageClient#Call('LanguageClient_NCM2OnComplete', {
                \ 'ctx': a:context,
                \ }, v:null)
endfunction

function! LanguageClient#explainErrorAtPoint(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'buftype': &buftype,
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('languageClient/explainErrorAtPoint', l:params, l:Callback)
endfunction

let g:LanguageClient_omniCompleteResults = []
function! LanguageClient#omniComplete(...) abort
    try
        " Note: do not add 'text' as it might be huge.
        let l:params = {
                    \ 'filename': LSP#filename(),
                    \ 'line': LSP#line(),
                    \ 'character': LSP#character(),
                    \ 'complete_position': v:null,
                    \ 'handle': v:false,
                    \ }
        call extend(l:params, get(a:000, 0, {}))
        let l:Callback = get(a:000, 1, g:LanguageClient_omniCompleteResults)
        call LanguageClient#Call('languageClient/omniComplete', l:params, l:Callback)
    catch
        call add(l:Callback, [])
        call s:Debug(string(v:exception))
    endtry
endfunction

function! LanguageClient#get_complete_start(input) abort
    " echomsg a:input
    return match(a:input, '\k*$')
endfunction

function! LanguageClient_filterCompletionItems(item, base) abort
    return a:item.word =~# '^' . a:base
endfunction

let g:LanguageClient_completeResults = []
function! LanguageClient#complete(findstart, base) abort
    if a:findstart
        " Before requesting completion, content between l:start and current cursor is removed.
        let s:completeText = LSP#text()

        let l:input = getline('.')[:LSP#character() - 1]
        let l:start = LanguageClient#get_complete_start(l:input)
        return l:start
    else
        " Magic happens that cursor jumps to the previously found l:start.
        let l:result = LanguageClient_runSync(
                    \ 'LanguageClient#omniComplete', {
                    \ 'character': LSP#character() + len(a:base),
                    \ 'complete_position': LSP#character(),
                    \ 'text': s:completeText,
                    \ })
        let l:result = l:result is v:null ? [] : l:result
        let l:filtered_items = []
        for l:item in l:result
            if LanguageClient_filterCompletionItems(l:item, a:base)
                call add(l:filtered_items, l:item)
            endif
        endfor
        return filtered_items
    endif
endfunction

function! LanguageClient#textDocument_signatureHelp(...) abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': v:true,
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:Callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('textDocument/signatureHelp', l:params, l:Callback)
endfunction

function! LanguageClient#workspace_applyEdit(...) abort
    let l:params = {
                \ 'edit': {},
                \ }
    call extend(l:params, a:0 >= 1 ? a:1 : {})
    let l:Callback = a:0 >= 2 ? a:2 : v:null
    return LanguageClient#Call('workspace/applyEdit', l:params, l:Callback)
endfunction

function! LanguageClient#workspace_executeCommand(command, ...) abort
    let l:params = {
                \ 'command': a:command,
                \ 'arguments': get(a:000, 0, v:null),
                \ }
    let l:Callback = get(a:000, 1, v:null)
    return LanguageClient#Call('workspace/executeCommand', l:params, l:Callback)
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
        let l:params = {
                \ 'method': '$cquery/base',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#cquery_callers(...) abort
    let l:params = {
                \ 'method': '$cquery/callers',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#cquery_vars(...) abort
    let l:params = {
                \ 'method': '$cquery/vars',
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return call('LanguageClient#findLocations', [l:params] + a:000[1:])
endfunction

function! LanguageClient#java_classFileContent(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, v:null)
    return LanguageClient#Call('java/classFileContent', l:params, l:Callback)
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
                \ 'Document Highlight': 'LanguageClient#textDocument_documentHighlight',
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

function! LanguageClient#debugInfo(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, v:null)
    return LanguageClient#Call('languageClient/debugInfo', l:params, l:Callback)
endfunction

let g:LanguageClient_loaded = s:Launch()
