" Wrap up remote plugin function, as right now remote plugin function cannot
" used as Funcref.
function! LanguageClient#FZFSinkTextDocumentDocumentSymbol(line) abort
    call LanguageClient_FZFSinkTextDocumentDocumentSymbol(a:line)
endfunction

function! LanguageClient#FZFSinkTextDocumentReferences(line) abort
    call LanguageClient_FZFSinkTextDocumentReferences(a:line)
endfunction

function! LanguageClient#FZFSinkWorkspaceSymbol(line) abort
    call LanguageClient_FZFSinkWorkspaceSymbol(a:line)
endfunction

function! LanguageClient#FZFSinkTextDocumentCodeAction(line) abort
    call LanguageClient_FZFSinkTextDocumentCodeAction(a:line)
endfunction

function! LanguageClient#complete(findstart, base) abort
    if a:findstart
        let l:line = getline('.')
        let l:col = col('.')
        if l:line[l:col - 2] =~ '\k'
            " Identifier - find it's beginning
            return matchend(l:line[:l:col - 1], '\v.*<')
        else
            " Not identifier - complete from here
            return l:col
        endif
    else
        call LanguageClient_textDocument_completionOmnifunc({'completeFromColumn': col('.')})
    endif
endfunction
