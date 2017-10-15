if has('nvim')
    finish
endif

let s:LanguageClient = yarp#py3('LanguageClient_wrap')

function! LanguageClientStart()
    return s:LanguageClient
endfunction
