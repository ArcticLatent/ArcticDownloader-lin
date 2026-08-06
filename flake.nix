{
  description = "Arctic ComfyUI Helper";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        config.allowUnfree = true;
      };
    in
    {
      packages.${system} = rec {
        arctic-comfyui-helper = pkgs.callPackage ./packaging/nix/source-package.nix { };
        default = arctic-comfyui-helper;
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/arctic-comfyui-helper";
        meta.description = "Run Arctic ComfyUI Helper";
      };

      devShells.${system} = {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = with pkgs; [
            appstream
            cargo
            cargo-tauri
            clippy
            flatpak
            flatpak-builder
            rustc
            rustfmt
          ];
        };

        # Windows cross-check shell for Linux/NixOS developers. The actual
        # release is still built by a native Windows GitHub Actions runner.
        windows = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            cargo-xwin
            clang
            lld
            llvm
            cacert
          ];
        };
      };
    };
}
