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
    rustup
    rust-analyzer # For integration tests.
    tmux
    vim # For manual tests.
    vim-vint
    (with python37Packages; [
      flake8
      pynvim
      pytest
    ])
  ])
  ++ stdenv.lib.optionals stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
    CoreServices
  ])
  ++ stdenv.lib.optionals stdenv.isLinux (with pkgs; [
    cargo-release  # build error on macOS: "iconv.h" not found.
  ]);

  NIX_LDFLAGS = stdenv.lib.optionalString stdenv.isDarwin "-framework CoreFoundation";
}
