function! LSP#filename() abort
    " When executing autocommand, `%` might have already changed.
    let l:filename = expand('<afile>:p')
    if !l:filename
        let l:filename = expand('%:p')
    endif
    return l:filename
endfunction

function! LSP#text(...) abort
    let l:buf = get(a:000, 0, '')

    let l:lines = getbufline(l:buf, 1, '$')
    if len(l:lines) > 0 && l:lines[-1] !=# '' && &fixendofline
        let l:lines += ['']
    endif
    return l:lines
endfunction

function! LSP#line() abort
    return line('.') - 1
endfunction

function! LSP#character() abort
    return col('.') - 1
endfunction

function! LSP#range_start_line() abort
    let l:lnum = v:lnum ? v:lnum : getpos("'<")[1]
    return l:lnum - 1
endfunction

function! LSP#range_end_line() abort
    if v:lnum
        return v:lnum - 1 + v:count
    endif

    return getpos("'>")[1]
endfunction

function! LSP#visible_line_start() abort
    return line('w0') - 1
endfunction

function! LSP#visible_line_end() abort
    return line('w$') - 1
endfunction

function! LSP#viewport() abort
    return {
        \ 'start': LSP#visible_line_start(),
        \ 'end': LSP#visible_line_end() + 1,
        \ }
endfunction

function! LSP#position() abort
	return {
		\ 'line': LSP#line(),
		\ 'character': LSP#character(),
		\ }
endfunction
