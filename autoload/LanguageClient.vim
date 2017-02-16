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
