" TODO: make buffer aware.

function! LSP#filename() abort
    " When executing autocommand, `%` might have already changed.
    let l:filename = expand('<afile>:p')
    if !l:filename
        let l:filename = expand('%:p')
    endif
    return l:filename
endfunction

" This function will return buffer text as required by LSP.
"
" The main difference with getbufline is that it checks fixendofline settings
" and add extra line at ending if appropriate.
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

function! LSP#viewport() abort
    return {
        \ 'start': line('w0') - 1,
        \ 'end': line('w$'),
        \ }
endfunction

function! LSP#position() abort
  return s:get_position()
endfunction

function! LSP#range(mode) abort
  if a:mode ==# 'v'
    return s:get_visual_selection()
  endif

  return {
    \ 'start': s:get_position(),
    \ 'end': s:get_position(),
    \ }
endfunction

function! s:get_position() abort
	return {
		\ 'line': LSP#line(),
		\ 'character': LSP#character(),
		\ }
endfunction

function! s:get_visual_selection() abort
    let [line_start, column_start] = getpos("'<")[1:2]
    let [line_end, column_end] = getpos("'>")[1:2]
    let lines = getline(line_start, line_end)
    if len(lines) == 0
        echohl Error | echom '[LC] No lines found in range.' | echohl None
        return ''
    endif
    let lines[-1] = lines[-1][: column_end - 2]
    let lines[0] = lines[0][column_start - 1:]

    return {
      \ 'start': {
        \ 'line': line_start - 1,
        \ 'character': column_start - 1,
        \ },
      \ 'end': {
        \ 'line': line_end - 1,
        \ 'character': column_end - 1,
        \ }
      \ }
endfunction
