let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs { };
  inherit (pkgs) stdenv;
in
pkgs.mkShell {
  buildInputs = (with pkgs; [
    curl
    git
    mypy
    neovim
    # rustup    # error on Linux: /lib64/libc.so.6: version `GLIBC_2.14' not found
    rust-analyzer
    tmux
    vim-vint
    (with python37Packages; [
      flake8
      pynvim
      pytest
    ])
  ])
  ++ stdenv.lib.optionals stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
    CoreServices
  ]);

  NIX_LDFLAGS = stdenv.lib.optionalString stdenv.isDarwin "-framework CoreFoundation";
}
