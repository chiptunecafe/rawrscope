{ pkgs ? import <nixpkgs> {} }:
  pkgs.mkShell {
    buildInputs = [
      (pkgs.latest.rustChannels.stable.rust.override { extensions = [ "rust-src" ]; })
      pkgs.clang
      pkgs.pkg-config
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXrandr
      pkgs.xorg.libXi
      pkgs.alsaLib
      pkgs.cmake
      pkgs.python3
      pkgs.llvm
      pkgs.llvmPackages.libclang
      pkgs.gnome3.zenity
      pkgs.openssl
    ];
    LIBCLANG_PATH = pkgs.lib.makeLibraryPath [ pkgs.llvmPackages.libclang ];
    RUST_SRC_PATH = "${pkgs.latest.rustChannels.stable.rust-src}/lib/rustlib/src/rust/src";
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.xorg.libXrandr pkgs.xorg.libXi ];
  }

