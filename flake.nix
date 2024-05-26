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

          src = nix-gitignore.gitignoreSource [ ] ./.;
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
        default = self.packages.${system}.eclssd;

        eclssd-cross-armv7l-linux =
          build-eclssd pkgsCross.armv7l-hf-multiplatform;
        eclssd-cross-aarch64-linux =
          build-eclssd pkgsCross.aarch64-multiplatform;
        eclssd-cross-pi = build-eclssd pkgsCross.raspberryPi;
      });

      ########################################################################
      #### Dev shell (for `nix develop`)
      ########################################################################
      devShells = forAllSystems
        (pkgs: with pkgs; let flakePkgs = self.packages.${system}; in {
          default = with flakePkgs; mkShell {
            buildInputs = [
              eclssd.buildInputs
              patchelf
              # eclssd-cross-armv7l-linux.buildInputs
              # eclssd-cross-aarch64-linux.buildInputs
              # eclssd-cross-pi.buildInputs
            ];
            nativeBuildInputs = [
              eclssd.nativeBuildInputs
              # eclssd-cross-armv7l-linux.nativeBuildInputs
              # eclssd-cross-aarch64-linux.nativeBuildInputs
              # eclssd-cross-pi.nativeBuildInputs
            ];
          };
        });

      nixosModules.default = { config, lib, pkgs, ... }: with lib; let
        cfg = config.services.eclssd;
      in
      {
        options.services.eclssd = with types; {
          enable = mkEnableOption "eclssd";
          logFilter = mkOption {
            type = separatedString ",";
            default = "info";
            example = "info,eclss=debug";
            description = "`tracing-subscriber` log filtering configuration for eclssd";
          };
          i2cdev = mkOption {
            type = path;
            default = "/dev/i2c-1";
            example = "/dev/i2c-1";
            description = "The I2C device to use for communication with sensors.";
          };
          server = {
            addr = mkOption {
              type = uniq str;
              default = "127.0.0.1";
              example = "127.0.0.1";
              description = "The address to bind the server on.";
            };

            port = mkOption {
              type = uniq port;
              default = 4200;
              example = 4200;
              description = "The port to bind the server on.";
            };
          };
        };

        config = mkIf cfg.enable {
          systemd.services.eclssd = {
            description = "Environmental Controls and Life Support Systems daemon";
            wantedBy = [ "multi-user.target" ];
            after = [ "networking.target" ];
            environment = {
              ECLSS_LOG = cfg.logFilter;
            };
            serviceConfig = {
              ExecStart = ''
                ${self.packages.${pkgs.system}.default}/bin/eclssd \
                  --i2cdev ${cfg.i2cdev} \
                  --listen-addr "${cfg.server.addr}:${toString cfg.server.port}"
              '';
              Restart = "on-failure";
              RestartSec = "5s";
              DynamicUser = lib.mkDefault true;
            };
          };
        };
      };
    };
}
