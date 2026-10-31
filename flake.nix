{
  description = "Terminal-based force-directed graph visualizer for markdown wikilinks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ] (
      system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "graf";
          version = (pkgs.lib.importTOML ./Cargo.toml).package.version;

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          doInstallCheck = true;
          installCheckPhase = ''
            runHook preInstallCheck
            $out/bin/graf --help > /dev/null
            runHook postInstallCheck
          '';

          meta = with pkgs.lib; {
            description = "Terminal-based force-directed graph visualizer for markdown wikilinks";
            homepage = "https://github.com/reekta92/graf";
            license = licenses.gpl3Only;
            mainProgram = "graf";
          };
        };

        apps.default = {
          type = "app";
          program = pkgs.lib.getExe self.packages.${system}.default;
          inherit (self.packages.${system}.default) meta;
        };

        checks.package = self.packages.${system}.default;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
          ];
        };
      }
    );
}
