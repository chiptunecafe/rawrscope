{
  description = "isms_constraint_solver shell";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system: let
        pkgs = import nixpkgs { inherit system; overlays = [ rust-overlay.overlay ]; };
      in {
        devShell = pkgs.mkShell {
          buildInputs = with pkgs.rust-bin.stable.latest; [
            (rust.override { extensions = [ "rust-src" ]; })
            pkgs.clang
            pkgs.pkg-config
            pkgs.cmake
            pkgs.python3
            pkgs.alsaLib
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.vulkan-loader
          ];
        };
      }
    );
}
