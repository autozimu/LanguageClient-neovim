function! LSP#filename() abort
    return expand('%:p')
endfunction

function! LSP#text() abort
    let l:lines = getline(1, '$')
    if l:lines[-1] !=# '' && &fixendofline
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
