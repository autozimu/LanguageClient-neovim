function! LSP#filename()
    return expand('%:p')
endfunction

function! LSP#text()
    let l:lines = getline(1, '$')
    if l:lines[-1] !=# '' && &fixendofline
        let l:lines += ['']
    endif
    return l:lines
endfunction

function! LSP#line()
    return line('.') - 1
endfunction

function! LSP#character()
    return col('.') - 1
endfunction
