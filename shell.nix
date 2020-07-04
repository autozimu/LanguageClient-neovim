let
  sources = import ./nix/sources.nix;
  pkgs = import sources.nixpkgs { };
  inherit (pkgs) stdenv;
in
pkgs.mkShell {
  buildInputs = (with pkgs; [
    mypy
    (with python37Packages; [
      flake8
      pynvim
      pytest
    ])
    rustup
    vim-vint
  ])
  ++ stdenv.lib.optionals stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
    CoreServices
  ]);

  NIX_LDFLAGS = stdenv.lib.optionalString stdenv.isDarwin "-framework CoreFoundation";
}
