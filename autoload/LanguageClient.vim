if get(g:, 'LanguageClient_loaded')
    finish
endif

let s:TYPE = {
\   'string':  type(''),
\   'list':    type([]),
\   'dict':    type({}),
\   'funcref': type(function('call'))
\ }
let s:FLOAT_WINDOW_AVAILABLE = exists('*nvim_open_win')
let s:POPUP_WINDOW_AVAILABLE = exists('*popup_atcursor')

" timers to control throttling
let s:timers = {}

if !hlexists('LanguageClientCodeLens')
  hi link LanguageClientCodeLens Title
endif

if !hlexists('LanguageClientWarningSign')
  hi link LanguageClientWarningSign todo
endif

if !hlexists('LanguageClientWarning')
  hi link LanguageClientWarning SpellCap
endif

if !hlexists('LanguageClientInfoSign')
  hi link LanguageClientInfoSign LanguageClientWarningSign
endif

if !hlexists('LanguageClientInfo')
  hi link LanguageClientInfo LanguageClientWarning
endif

if !hlexists('LanguageClientErrorSign')
  hi link LanguageClientErrorSign error
endif

if !hlexists('LanguageClientError')
  hi link LanguageClientError SpellBad
endif


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

function! s:getSelectionUI() abort
	if type(get(g:, 'LanguageClient_selectionUI', v:null)) is s:TYPE.funcref
		return 'funcref'
	else
		return get(g:, 'LanguageClient_selectionUI', v:null)
	endif
endfunction

function! s:useVirtualText() abort
    let l:use = s:GetVar('LanguageClient_useVirtualText')
    if l:use isnot v:null
        return l:use
    endif

    if exists('*nvim_buf_set_virtual_text')
        return 'All'
    else
        return 'No'
    endif
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

    let l:prefix = s:GetVar('LanguageClient_virtualTextPrefix')
    if l:prefix is v:null
        let l:prefix = ''
    endif

    if !exists('*nvim_buf_set_virtual_text')
        return
    endif

    call nvim_buf_clear_namespace(a:buf_id, a:ns_id, a:line_start, a:line_end)

    for vt in a:virtual_texts
        call nvim_buf_set_virtual_text(a:buf_id, a:ns_id, vt['line'], [[l:prefix . vt['text'], vt['hl_group']]], {})
    endfor
endfunction

function! s:place_sign(id, name, file, line) abort
  if !exists('*sign_place')
    execute 'sign place id=' . a:id . ' name=' . a:name . ' file=' . a:file . ' line=' . a:line
  endif

  call sign_place(0, 'LanguageClientNeovim', a:name, a:file, { 'lnum': a:line })
endfunction

" clears all signs on the buffer with the given name
function! s:clear_buffer_signs(file) abort
  if !exists('*sign_unplace')
    execute 'sign unplace * group=LanguageClientNeovim buffer=' . a:file
  else
    call sign_unplace('LanguageClientNeovim', { 'buffer': a:file })
  endif
endfunction

" replaces the signs on a file with the ones passed as an argument
function! s:set_signs(file, signs) abort
  call s:clear_buffer_signs(a:file)

  for l:sign in a:signs
    let l:line = l:sign['line'] + 1
    let l:name = l:sign['name']
    let l:id = l:sign['id']
    call s:place_sign(l:id, l:name, a:file, l:line)
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

function! s:inputlist(...) abort
    call inputsave()
    let l:selection = inputlist(a:000)
    call inputrestore()
    return l:selection
endfunction

function! s:selectionUI_funcref(source, sink) abort
    if type(get(g:, 'LanguageClient_selectionUI')) is s:TYPE.funcref
        call call(g:LanguageClient_selectionUI, [a:source, function(a:sink)])
    elseif get(g:, 'LanguageClient_selectionUI', 'FZF') ==? 'FZF'
                \ && get(g:, 'loaded_fzf')
        call s:FZF(a:source, a:sink)
    else
        call s:Echoerr('Unsupported selection UI, use "FZF" or a funcref')
        return
    endif
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
    if has('nvim') && !has('nvim-0.4')
        call feedkeys('i')
    endif
endfunction

function! s:Edit(action, path) abort
    " If editing current file, push current location to jump list.
    let l:bufnr = bufnr(a:path)
    if l:bufnr == bufnr('%')
        execute 'normal! m`'
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

function! s:ApplySemanticHighlights(bufnr, ns_id, clears, highlights) abort
    " TODO: implement this for vim8
    if !has('nvim')
      return
    endif

    for clear in a:clears
        call nvim_buf_clear_namespace(a:bufnr, a:ns_id, clear.line_start, clear.line_end)
    endfor

    for hl in a:highlights
        call nvim_buf_add_highlight(a:bufnr, a:ns_id, hl.group, hl.line, hl.character_start, hl.character_end)
    endfor
endfunction

" Batch version of nvim_buf_add_highlight
function! s:AddHighlights(namespace, highlights) abort
  if has('nvim')
    let l:namespace_id = nvim_create_namespace(a:namespace)
    for hl in a:highlights
        call nvim_buf_add_highlight(0, l:namespace_id, hl.group, hl.line, hl.character_start, hl.character_end)
    endfor
  else
    let match_ids = []
    for hl in a:highlights
      let match_id = matchaddpos(hl.group, [[hl.line + 1, hl.character_start + 1, hl.character_end - hl.character_start]])
      let match_ids = add(match_ids, match_id)
    endfor

    call setbufvar(bufname(), a:namespace . '_IDS', match_ids)
  endif
endfunction

function! s:SetHighlights(highlights, namespace) abort
  call s:ClearHighlights(a:namespace)
  call s:AddHighlights(a:namespace, a:highlights)
endfunction

function! s:ClearHighlights(namespace) abort
  if has('nvim')
    let l:namespace_id = nvim_create_namespace(a:namespace)
    call nvim_buf_clear_namespace(0, l:namespace_id, 0, -1)
  else
    let match_ids = get(b:, a:namespace . '_IDS', [])
    for mid in match_ids
      " call inside a try/catch to avoid error for manually cleared matches
      try | call matchdelete(mid) | catch
      endtry
    endfor
    call setbufvar(bufname(), a:namespace . '_IDS', [])
  endif
endfunction

" Get an variable value.
" Get variable from buffer local, or else global, or else default, or else v:null.
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

" if the argument is a list, return it unchanged, otherwise return the list
" containing the argument.
function! s:ToList(x) abort
    if type(a:x) == v:t_list
        return a:x
    else
        return [ a:x ]
    endif
endfunction

function! s:ShouldUseFloatWindow() abort
    let floatingHoverEnabled = s:GetVar('LanguageClient_useFloatingHover', v:true)
    return s:FLOAT_WINDOW_AVAILABLE && floatingHoverEnabled
endfunction

function! s:CloseFloatingHover() abort
    if !exists('s:float_win_id')
        return
    endif

    autocmd! plugin-LC-neovim-close-hover
    let winnr = win_id2win(s:float_win_id)
    if winnr == 0
        return
    endif
    execute winnr . 'wincmd c'
endfunction

function! s:CloseFloatingHoverOnCursorMove(opened) abort
    if getpos('.') == a:opened
        " Just after opening floating window, CursorMoved event is run.
        " To avoid closing floating window immediately, check the cursor
        " was really moved
        return
    endif
    autocmd! plugin-LC-neovim-close-hover
    let winnr = win_id2win(s:float_win_id)
    if winnr == 0
        return
    endif
    execute winnr . 'wincmd c'
endfunction

function! s:CloseFloatingHoverOnBufEnter(bufnr) abort
    let winnr = win_id2win(s:float_win_id)
    if winnr == 0
        " Float window was already closed
        autocmd! plugin-LC-neovim-close-hover
        return
    endif
    if winnr == winnr()
        " Cursor is moving into floating window. Do not close it
        return
    endif
    if bufnr('%') == a:bufnr
        " When current buffer opened hover window, it's not another buffer. Skipped
        return
    endif
    autocmd! plugin-LC-neovim-close-hover
    execute winnr . 'wincmd c'
endfunction

" Open preview window. Window is open in:
"   - Floating window on Neovim (0.4.0 or later)
"   - popup window on vim (8.2 or later)
"   - Preview window on Neovim (0.3.0 or earlier) or Vim
"
" Receives two optional arguments which are the X and Y position 
function! s:OpenHoverPreview(bufname, lines, filetype, ...) abort
    " Use local variable since parameter is not modifiable
    let lines = a:lines
    let bufnr = bufnr('%')

    let display_approach = ''
    if s:ShouldUseFloatWindow()
        let display_approach = 'float_win'
    elseif s:POPUP_WINDOW_AVAILABLE && s:GetVar('LanguageClient_usePopupHover', v:true)
        let display_approach = 'popup_win'
    else
        let display_approach = 'preview'
    endif

    if display_approach ==# 'float_win'
        " When a language server takes a while to initialize and the user
        " calls hover multiple times during that time (for example, via an
        " automatic hover on cursor move setup), we will get a number of
        " successive calls into this function resulting in many hover windows
        " opened. This causes a number of issues, and we only really want one,
        " so make sure that the previous hover window is closed.
        call s:CloseFloatingHover()

        let pos = getpos('.')
        let l:hoverMarginSize = s:GetVar('LanguageClient_hoverMarginSize', 1)
        " Calculate width and height and give margin to lines
        let width = 0
        for index in range(len(lines))
            let line = lines[index]
            if line !=# ''
                " Give a left margin
                let line = repeat(' ', l:hoverMarginSize) . line
            endif
            let lw = strdisplaywidth(line)
            if lw > width
                let width = lw
            endif
            let lines[index] = line
        endfor

        " Give margin
        let width += l:hoverMarginSize
        let l:topBottom = repeat([''], l:hoverMarginSize)
        let lines = l:topBottom + lines + l:topBottom
        let height = len(lines)

        " Calculate anchor
        " Prefer North, but if there is no space, fallback into South
        let bottom_line = line('w0') + winheight(0) - 1
        if pos[1] + height <= bottom_line
            let vert = 'N'
            let row = 1
        else
            let vert = 'S'
            let row = 0
        endif

        " Prefer West, but if there is no space, fallback into East
        if pos[2] + width <= &columns
            let hor = 'W'
            let col = 0
        else
            let hor = 'E'
            let col = 1
        endif

        let relative = 'cursor'
        let col = get(a:000, 0, col)
        let row = get(a:000, 1, row)
        if get(a:000, 0, v:null) isnot v:null && get(a:000, 1, v:null) isnot v:null
          let relative = 'win'
        endif

        let s:float_win_id = nvim_open_win(bufnr, v:true, {
        \   'relative': relative,
        \   'anchor': vert . hor,
        \   'row': row,
        \   'col': col,
        \   'width': width,
        \   'height': height,
        \   'style': s:GetVar('LanguageClient_floatingWindowStyle', 'minimal'),
        \ })

        execute 'noswapfile edit!' a:bufname

        let float_win_highlight = s:GetVar('LanguageClient_floatingHoverHighlight', 'Normal:CursorLine')
        execute printf('setlocal winhl=%s', float_win_highlight)
    elseif display_approach ==# 'popup_win'
        let l:padding = [1, 1, 1, 1]
        if get(a:000, 0, v:null) isnot v:null && get(a:000, 1, v:null) isnot v:null
          let pop_win_id = popup_create(a:lines, {
                \ 'line': get(a:000, 1) + 1,
                \ 'col': get(a:000, 0) + 1,
                \ 'padding': l:padding,
                \ 'moved': 'any'
                \ })
        else
          let pop_win_id = popup_atcursor(a:lines, { 'padding': l:padding })
        endif
        call setbufvar(winbufnr(pop_win_id), '&filetype', a:filetype)
        " trigger refresh on plasticboy/vim-markdown
        call win_execute(pop_win_id, 'doautocmd InsertLeave')
    elseif display_approach ==# 'preview'
        execute 'silent! noswapfile pedit!' a:bufname
        wincmd P
    else
        call s:Echoerr('Unknown display approach: ' . display_approach)
    endif

    if display_approach !=# 'popup_win'
        setlocal buftype=nofile nobuflisted bufhidden=wipe nonumber norelativenumber signcolumn=no modifiable

        if a:filetype isnot v:null
            let &filetype = a:filetype
        endif

        call setline(1, lines)
        " trigger refresh on plasticboy/vim-markdown
        doautocmd InsertLeave
        setlocal nomodified nomodifiable

        wincmd p
    endif

    if display_approach ==# 'float_win'
        " Unlike preview window, :pclose does not close window. Instead, close
        " hover window automatically when cursor is moved.
        let call_after_move = printf('<SID>CloseFloatingHoverOnCursorMove(%s)', string(pos))
        let call_on_bufenter = printf('<SID>CloseFloatingHoverOnBufEnter(%d)', bufnr)
        augroup plugin-LC-neovim-close-hover
            execute 'autocmd CursorMoved,CursorMovedI,InsertEnter <buffer> call ' . call_after_move
            execute 'autocmd BufEnter * call ' . call_on_bufenter
        augroup END
    endif
endfunction

function! s:MoveIntoHoverPreview(bufname) abort
    for bufnr in range(1, bufnr('$'))
        if bufname(bufnr) ==# a:bufname
            let winnr = bufwinnr(bufnr)
            if winnr != -1
                execute winnr . 'wincmd w'
            endif
            return v:true
        endif
    endfor
    return v:false
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
        if type(a:lines) == type(0) && (a:lines == 0 || a:lines == 143)
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
    let l:path = s:GetVar('LanguageClient_binaryPath')
    if l:path isnot v:null
        return l:path
    endif

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
        " abort write if nvim is exiting
        if get(v:, 'exiting', v:null) isnot v:null
          return
        endif

        " jobsend respond 1 for success.
        return !jobsend(s:job, l:message)
    elseif has('channel')
        return ch_sendraw(s:job, l:message)
    else
        echoerr 'Not supported: not nvim nor vim with +channel.'
    endif
endfunction

function! s:SkipSendingMessage() abort
    if expand('%') =~# '^jdt://'
        return v:false
    endif

    let l:has_command = LanguageClient#HasCommand(&filetype)
    return !l:has_command || &buftype !=# '' || &filetype ==# '' || expand('%') ==# ''
endfunction

function! LanguageClient#HasCommand(filetype) abort
  let l:commands = s:GetVar('LanguageClient_serverCommands', {})
  return has_key(l:commands, a:filetype)
endfunction

function! LanguageClient#Call(method, params, callback, ...) abort
    if s:SkipSendingMessage()
        echo '[LC] Server not configured for filetype ' . &filetype
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
    if s:SkipSendingMessage()
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
    if s:ShouldUseFloatWindow() && s:MoveIntoHoverPreview('__LCNHover__')
        return
    endif
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

function! LanguageClient#closeFloatingHover() abort
    call s:CloseFloatingHover()
endfunction

" Meta methods to go to various places.
function! LanguageClient#findLocations(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'position': LSP#position(),
                \ 'gotoCmd': v:null,
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('languageClient/findLocations', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_switchSourceHeader(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/switchSourceHeader', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_definition(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ 'gotoCmd': v:null,
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/definition', l:params, l:Callback)
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

function! LanguageClient#textDocument_codeLens(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'text': LSP#text(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('textDocument/codeLens', l:params, l:Callback)
endfunction

function! s:do_codeAction(mode, ...) abort
    let l:Callback = get(a:000, 2, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ 'range': LSP#range(a:mode),
                \ }
    call extend(l:params, get(a:000, 1, {}))
    return LanguageClient#Call('textDocument/codeAction', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_visualCodeAction(...) range abort
  call s:do_codeAction('v', a:000)
endfunction

function! LanguageClient#textDocument_codeAction(...) abort
  call s:do_codeAction('n', a:000)
endfunction

function! LanguageClient#executeCodeAction(kind, ...) abort
  let l:Callback = get(a:000, 1, v:null)
  let l:params = {
              \ 'filename': LSP#filename(),
              \ 'line': LSP#line(),
              \ 'character': LSP#character(),
              \ 'handle': s:IsFalse(l:Callback),
              \ 'range': LSP#range('n'),
              \ 'kind': a:kind,
              \ }
  call extend(l:params, get(a:000, 0, {}))
  return LanguageClient#Call('languageClient/executeCodeAction', l:params, l:Callback)
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
                \ 'range_start_line': LSP#range_start_line(),
                \ 'range_end_line': LSP#range_end_line(),
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
    call extend(l:params, get(a:000, 0, {})) " extend with pumpos params
    return LanguageClient#Call('completionItem/resolve', l:params, l:Callback)
endfunction

function! LanguageClient#textDocument_rangeFormatting_sync(...) abort
    let l:result = LanguageClient_runSync('LanguageClient#textDocument_rangeFormatting', {
                \ 'handle': v:true,
                \ })
    return l:result isnot v:null
endfunction

function! LanguageClient#textDocument_didOpen(...) abort
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
    return LanguageClient#Call('languageClient/startServer', l:params, funcref('LanguageClient#textDocument_didOpen'))
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

function! LanguageClient#diagnosticsPrevious() abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'position': LSP#position(),
                \ }
    return LanguageClient#Notify('languageClient/diagnosticsPrevious', l:params)
endfunction

function! LanguageClient#diagnosticsNext() abort
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'position': LSP#position(),
                \ }
    return LanguageClient#Notify('languageClient/diagnosticsNext', l:params)
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

function! LanguageClient#handleBufEnter() abort
    if !exists('b:LanguageClient_isServerRunning')
      let b:LanguageClient_isServerRunning = 0
    endif

    if !exists('b:LanguageClient_statusLineDiagnosticsCounts')
      let b:LanguageClient_statusLineDiagnosticsCounts = {}
    endif

    try
        call LanguageClient#Notify('languageClient/handleBufEnter', {
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
                        \ 'position': LSP#position(),
                        \ 'viewport': LSP#viewport(),
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

" TODO: Separate CursorMoved and ViewportChanged events. But after separating,
" there will Mutex poison error.
let s:last_cursor_line = -1
function! LanguageClient#handleCursorMoved() abort
  call s:timer_stop('LanguageClient#handleCursorMoved')

  function! DebounceHandleCursorMoved() abort
    let l:cursor_line = getcurpos()[1] - 1
    if l:cursor_line == s:last_cursor_line
        return
    endif
    let s:last_cursor_line = l:cursor_line

    try
        call LanguageClient#Notify('languageClient/handleCursorMoved', {
                    \ 'buftype': &buftype,
                    \ 'filename': LSP#filename(),
                    \ 'position': LSP#position(),
                    \ 'viewport': LSP#viewport(),
                    \ })
    catch
        call s:Debug('LanguageClient caught exception: ' . string(v:exception))
    endtry
  endfunction

  call s:timer_start_store(100, { -> DebounceHandleCursorMoved() }, 'LanguageClient#handleCursorMoved')
endfunction

function! LanguageClient#handleCompleteDone() abort
    " close any hovers that may have been opened for example for completion
    " item documentation.
    call s:ClosePopups()

    let user_data = get(v:completed_item, 'user_data', '')
    if len(user_data) ==# 0
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
    let extra = get(a:000, 0, {})
    let silent_mode = get(extra, 'silent', v:false)
    if s:ShouldUseFloatWindow() && !silent_mode && s:MoveIntoHoverPreview('__LCNExplainError__')
        return
    endif

    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'buftype': &buftype,
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ 'handle': s:IsFalse(l:Callback),
                \ }
    call extend(l:params, extra)
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

function! s:shutdownCallback(...) abort
    call LanguageClient#exit()
    echom '[LC] Server shutdown complete'
endfunction

function! LanguageClient#shutdown() abort
    return LanguageClient#Call('shutdown', {
                \ 'languageId': &filetype,
                \ },
                \ function('s:shutdownCallback'))
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

function! LanguageClient#isServerRunning() abort
    return get(b:, 'LanguageClient_isServerRunning', 0)
endfunction

" Example function usable for status line.
function! LanguageClient#statusLine() abort
    if g:LanguageClient_serverStatusMessage ==# ''
        return ''
    endif

    return '[' . g:LanguageClient_serverStatusMessage . ']'
endfunction

function! LanguageClient#statusLineDiagnosticsCounts() abort
    return b:LanguageClient_statusLineDiagnosticsCounts
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

function! LanguageClient#java_classFileContents(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, v:null)
    return LanguageClient#Call('java/classFileContents', l:params, l:Callback)
endfunction

function! LanguageClient#handleCodeLensAction(...) abort
    let l:Callback = get(a:000, 1, v:null)
    let l:params = {
                \ 'filename': LSP#filename(),
                \ 'line': LSP#line(),
                \ 'character': LSP#character(),
                \ }
    call extend(l:params, get(a:000, 0, {}))
    return LanguageClient#Call('LanguageClient/handleCodeLensAction', l:params, l:Callback)
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
    let l:useSelectionUI = get(g:, 'LanguageClient_selectionUIContextMenu',
				\ get(g:, 'LanguageClient_fzfContextMenu', 1))
    if l:useSelectionUI
            \ && (type(get(g:, 'LanguageClient_selectionUI', v:null)) is s:TYPE.funcref
                \ || (get(g:, 'LanguageClient_selectionUI', 'FZF') ==? 'FZF'
                    \ && get(g:, 'loaded_fzf')
                \ )
            \ )
        return s:selectionUI_funcref(l:options, function('LanguageClient_handleContextMenuItem'))
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

function! LanguageClient_showSemanticScopes(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, function('s:print_semantic_scopes'))

    return LanguageClient#Call('languageClient/semanticScopes', l:params, l:Callback)
endfunction

function! s:print_semantic_scopes(response) abort
    let l:scope_mappings = a:response.result

    let l:msg = ''
    for mapping in l:scope_mappings
        let l:msg .= "Highlight Group:\n"
        let l:msg .= ' ' . l:mapping.hl_group . "\n"

        let l:msg .= "Semantic Scope:\n"
        let l:spaces = ' '
        for l:scope_name in l:mapping.scope
            let l:msg .= l:spaces . l:scope_name . "\n"
            let l:spaces .= ' '
        endfor
        let l:msg .= "\n"
    endfor

    echo l:msg
endfunction

function! LanguageClient#showSemanticHighlightSymbols(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, v:null)

    return LanguageClient#Call('languageClient/showSemanticHighlightSymbols', l:params, l:Callback)
endfunction

function! LanguageClient_showCursorSemanticHighlightSymbols(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, function('s:print_cursor_semantic_symbol'))

    return LanguageClient#showSemanticHighlightSymbols(l:params, l:Callback)
endfunction

function! s:print_cursor_semantic_symbol(response) abort
    let l:symbols = a:response.result
    let l:lines = []

    for symbol in l:symbols
        if l:symbol.line + 1 == line('.') &&
                    \ symbol.character_start < col('.') &&
                    \ col('.') <= symbol.character_end
            let l:spaces = ''
            for scope_name in l:symbol.scope
                call add(l:lines, l:spaces . l:scope_name)
                let l:spaces .= ' '
            endfor
        endif
    endfor

    if len(l:lines) > 0
        call s:OpenHoverPreview('SemanticScopes', l:lines, 'text')
    else
        call s:Echowarn('No Symbol Under Cursor or No Semantic Highlighting')
    endif
endfunction

function! LanguageClient#debugInfo(...) abort
    let l:params = get(a:000, 0, {})
    let l:Callback = get(a:000, 1, v:null)
    return LanguageClient#Call('languageClient/debugInfo', l:params, l:Callback)
endfunction

function! s:ClosePopups(...) abort
  if s:ShouldUseFloatWindow()
    call s:CloseFloatingHover()
  elseif exists('*popup_clear') && s:GetVar('LanguageClient_usePopupHover', v:true)
    call popup_clear()
  else
    :pclose
  endif
endfunction

" receives the v:event from the CompleteChanged autocmd
function! LanguageClient#handleCompleteChanged(event) abort
  " this function needs timer_start because by the time it is called the
  " `textlock` lock is set, so calling something (ClosePopups in this case) in
  " a timer basically unsets that lock.
  if !exists('*timer_start')
    return
  endif

  " this timer is just to stop textlock from locking our changes
  call s:timer_start(0, funcref('s:ClosePopups'))
  call s:timer_stop('LanguageClient#handleCompleteChanged')

  function! DebounceHandleCompleteChanged(event) abort
    let l:user_data = get(v:completed_item, 'user_data', '')
    if len(l:user_data) ==# 0
      return
    endif

    if type(l:user_data) ==# v:t_string
      let l:user_data = json_decode(l:user_data)
    endif

    let l:completed_item = {}

    " LCN completion items
    if has_key(l:user_data, 'lspitem')
      let l:completed_item = l:user_data['lspitem']
    endif

    " NCM2 completion items
    if has_key(l:user_data, 'ncm2_lspitem')
      let l:completed_item = l:user_data['ncm2_lspitem']
    endif

    if l:completed_item ==# {}
      return
    endif

    if has_key(l:completed_item, 'documentation')
      call s:ShowCompletionItemDocumentation(l:completed_item['documentation'], a:event)
    else
      call LanguageClient#completionItem_resolve(l:completed_item, { 'pumpos': a:event })
    endif
  endfunction

  call s:timer_start_store(100, { -> DebounceHandleCompleteChanged(a:event) }, 'LanguageClient#handleCompleteChanged')
endfunction

function! s:ShowCompletionItemDocumentation(doc, completion_event) abort
  let l:kind = 'text'

  " some servers send a dictionary with kind and value whereas others just
  " send the value
  if type(a:doc) is s:TYPE.dict
    let l:lines = split(a:doc['value'], "\n")
    if has_key(a:doc, 'kind')
      let l:kind = a:doc['kind']
    endif
  else
    let l:lines = split(a:doc, "\n")
  endif

  if len(l:lines) ==# 0
    return
  endif

  for l:line in l:lines
    let l:line = ' ' . l:line . ' '
  endfor

  let l:pos = a:completion_event
  if exists('*pum_getpos')
    " favor pum_getpos output if available
    let l:pos = pum_getpos()
  endif
  let l:x_pos = l:pos['width'] + l:pos['col'] + 1
  call s:OpenHoverPreview('CompletionItemDocumentation', l:lines, l:kind, l:x_pos, l:pos['row'])
endfunction

" s:timer_stop tries to stop the timer with the given name by calling vim's
" timer_stop. If vim's timer_stop function does not exist it just returns.
function! s:timer_stop(name) abort
	if !exists('*timer_stop')
		return
	endif

	if has_key(s:timers, a:name)
  	call timer_stop(s:timers[a:name])
	endif
endfunction

" s:timer_start tries to start a timer by calling vim's timer_start function,
" if it does not exist it just calls the function given in the second
" argument.
function! s:timer_start(delay, func) abort
	if !exists('*timer_start')
		return a:func()
	endif

  return timer_start(a:delay, a:func)
endfunction

" s:timer_start_store calls s:timer_start and stores the returned timer_id in
" a script scoped s:timers variable that we can use to debounce function
" calls.
function! s:timer_start_store(delay, func, name) abort
  let s:timers[a:name] = s:timer_start(a:delay, a:func)
endfunction

let g:LanguageClient_loaded = s:Launch()
