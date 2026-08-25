{
  description = "fastcount - an incredibly fast, incredibly useless counter";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "fastcount";
          version = "2.1.0";

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              (baseNameOf path != ".cargo") && (pkgs.lib.cleanSourceFilter path type);
          };

          cargoLock.lockFile = ./Cargo.lock;

          postPatch = ''
            sed -i '/panic-immediate-abort/d' Cargo.toml
            sed -i 's/immediate-abort/abort/g' Cargo.toml
          '';

          meta = with pkgs.lib; {
            description = "An incredibly fast, incredibly useless counter";
            homepage = "https://github.com/CallMeAlphabet/fastcount";
            license = licenses.asl20;
            platforms = platforms.linux;
            mainProgram = "fastcount";
          };
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            (rustToolchain.override {
              extensions = [ "rust-src" "rust-analyzer" ];
            })
          ];
        };
      });
}
