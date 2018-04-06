function! s:checkJobFeature() abort
    if !has('nvim') && !has('job')
        call health#report_error('Not supported: not nvim nor vim with +job.')
    endif
endfunction

function! s:checkBinary() abort
    let l:path = LanguageClient#binaryPath()
    if executable(l:path) ==# 1
        call health#report_ok('binary found: ' . l:path)
    else
        call health#report_error(
                    \ 'binary is missing or not executable. ' .
                    \ 'Try reinstall it with install.sh or install.ps1: ' .
                    \ l:path)
    endif

    let output = system([l:path, '--version'])
    call health#report_ok(output)
endfunction

function! health#LanguageClient#check() abort
    call s:checkJobFeature()
    call s:checkBinary()
endfunction
