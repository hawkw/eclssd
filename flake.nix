{
  description = "debugger for Hubris";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-23.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "armv7l-linux" "x86_64-darwin" "aarch64-darwin" ];
      overlays = [ (import rust-overlay) ];
      forAllSystems = function:
        nixpkgs.lib.genAttrs systems
          (system:
            let
              pkgs = import nixpkgs {
                inherit system overlays;
              };
            in
            function pkgs);
      pname = "eclssd";
      build-eclssd =
        (pkgs: with pkgs; let

          # use the Rust toolchain specified in the project's rust-toolchain.toml
          rustToolchain =
            let
              file = pkgsBuildHost.rust-bin.fromRustupToolchainFile
                ./rust-toolchain.toml;
            in
            file.override {
              extensions = [
                "rust-src" # for rust-analyzer
              ];
            };

          configuredRustPlatform = makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };

          src = ./.;
          cargoTOML = lib.importTOML "${src}/eclssd/Cargo.toml";
        in
        configuredRustPlatform.buildRustPackage {
          inherit src pname;
          inherit (cargoTOML.package) version;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "linux-embedded-hal-0.4.0" = "sha256-2CxZcBMWaGP0DTiyQoDkwVFoNbEBojGySirSb2z+40U=";
              "scd4x-0.3.0" = "sha256-2pbYEDX2454lP2701eyhtRWu1sSW8RSStVt6iOF0fmI=";
              "sensirion-i2c-0.3.0" = "sha256-HS6anAmUBBrhlP/kBSv243ArnK3ULK8+0JA8kpe6LAk=";
              "tinymetrics-0.1.0" = "sha256-QUEZDLcx20b5S0CEPM9r6IMKRvVNkkylrliEP50cJKs=";
            };
          };
        });
    in
    {
      ########################################################################
      #### Packages
      ########################################################################
      packages = forAllSystems (pkgs: with pkgs; {
        eclssd = build-eclssd pkgs;
        default = self.packages.${system}.humility;

        eclssd-cross-armv7l-linux =
          build-eclssd pkgsCross.armv7l-hf-multiplatform;
        eclssd-cross-aarch64-linux =
          build-eclssd pkgsCross.aarch64-multiplatform;
      });

      ########################################################################
      #### Dev shell (for `nix develop`)
      ########################################################################
      devShells = forAllSystems
        (pkgs: with pkgs; let flakePkgs = self.packages.${system}; in {
          default = with flakePkgs; mkShell {
            buildInputs = [
              eclssd.buildInputs
              eclssd-cross-armv7l-linux.buildInputs
              eclssd-cross-aarch64-linux.buildInputs
            ];
            nativeBuildInputs = [
              eclssd.nativeBuildInputs
              eclssd-cross-armv7l-linux.nativeBuildInputs
              eclssd-cross-aarch64-linux.nativeBuildInputs
            ];
          };
        });


    };
}
