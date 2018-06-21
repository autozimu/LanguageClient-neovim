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
    return v:lnum - 1
endfunction

function! LSP#range_end_line() abort
    return v:lnum - 1 + v:count
endfunction
