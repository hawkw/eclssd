{
  description = "Environmental Controls and Life Support Systems";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-23.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
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
          buildInputs = with pkgs; [
            SDL2
            SDL2.dev
          ];

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "sensor-sen5x-0.1.0" = "sha256-G54U1/tb2QWUQbUtcnYvnzGs8XPW1tZhbh5QOR8hiBg=";
              "sgp30-0.3.2" = "sha256-e++4WnEACeuYla4kW405VThEOl36simMU9rrTMQA/I8=";
              "sht4x-0.2.0" = "sha256-5ST2EJyFym5lbVvAJj3VIWn5z3SC783nLvJYolfgtoo=";
              "tinymetrics-0.1.0" = "sha256-zt7lqq2sYY9k+bMQOepxRamg1CGynFafbxBJDISl1U8=";
              "bosch-bme680-1.0.2" = "sha256-g06bpJP3PgFF9peraYxr3pU5jzZrA8xL/D6+kwr/Nfc=";
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
        name = "eclssd";
        cfg = config.services.${name};
        ssd1680 = "ssd1680";
        # cfgCtl = config.programs.eclssctl;
        description = "Environmental Controls and Life Support Systems daemon";
      in
      {
        options = with types; {
          services.eclssd = {
            enable = mkEnableOption name;

            i2cdev = mkOption {
              type = path;
              default = "/dev/i2c-1";
              example = "/dev/i2c-1";
              description = "The I2C device to use for communication with sensors.";
            };

            openPorts = mkOption {
              type = bool;
              default = false;
              description = "Whether to open firewall ports for eclssd";
            };

            onlySensors = mkOption {
              type = listOf (enum [
                "BME680"
                "ENS160"
                "PMSA003I"
                "SCD30"
                "SCD40"
                "SCD41"
                "SHT41"
                "SGP30"
                "SEN55"
              ]);
              default = [ ];
              description = ''
                A list of sensors to explicitly enable, or an empty list to enable all supported sensors.

                If this is null, the ECLSS daemon will attempt to use all supported sensors.
              '';
            };

            # Currently this doesn't do anything but I intend to use it for my
            # Prometheus scrape config...
            location = mkOption {
              type = uniq str;
              default = "${config.networking.hostname}";
              example = "bedroom";
              description = "The physical location of this ECLSS sensor.";
            };

            logging = {
              filter = mkOption {
                type = separatedString ",";
                default = "info";
                example = "info,eclss=debug";
                description = "`tracing-subscriber` log filtering configuration for eclssd";
              };

              timestamps = mkEnableOption "timestamps in log output";

              colors = mkOption {
                type = bool;
                default = true;
                example = false;
                description = "Whether to enable ANSI color codes in log output.";
              };

              format = mkOption {
                type = enum [ "text" "json" "journald" ];
                default = "text";
                example = "json";
                description = "The log output format.";
              };
            };

            server = {
              addr = mkOption {
                type = uniq str;
                default = "0.0.0.0";
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

            readoutd = {
              ${ssd1680} = {
                enable = mkEnableOption "SSD1680 display support";
              };
            };
          };
        };

        config = let eclssPkg = self.packages.${pkgs.system}.default; in mkIf cfg.enable (mkMerge [
          {
            # eclssd user/group. the service requires its own user in order to
            # add the "i2c" group.
            users = {
              users.${name} = {
                inherit description;
                isSystemUser = true;
                group = name;
                extraGroups = [ "i2c" ];
              };
              groups.${name} = { };
            };

            services.udev.extraRules = ''
              SUBSYSTEM=="i2c-dev", TAG+="systemd"
            '';

            environment.systemPackages = [ eclssPkg ];

            systemd.services.${name} =
              let
                sensorArgs = strings.concatMapStrings (sensor: " --sensor ${sensor}") cfg.onlySensors;
              in
              {
                inherit description;
                wantedBy = [ "multi-user.target" ];
                after = [ "networking.target" ];
                environment = {
                  ECLSS_LOG = cfg.logging.filter;
                  ECLSS_LOG_FORMAT = cfg.logging.format;
                  ECLSS_LOCATION = cfg.location;
                };
                serviceConfig = {
                  User = name;
                  Group = name;
                  ExecStart = ''${eclssPkg}/bin/${name} \
                    --i2cdev '${cfg.i2cdev}' \
                    --listen-addr '${cfg.server.addr}:${toString cfg.server.port}'\
                    ${sensorArgs}
                  '';
                  Restart = "on-failure";
                  RestartSec = "5s";
                  # only start if the I2C adapter is up.
                  # ConditionPathExists = "/sys/class/i2c-adapter";
                  # Ensure that the "API VFS" (i.e. /dev/i2c-n) is mounted for
                  # the service.
                  MountAPIVFS = true;
                  # Ensure the system has access to real hardware devices in
                  # /dev
                  PrivateDevices = false;
                  # Ensure the service has access to the network so that it can
                  # bind its listener.
                  PrivateNetwork = false;
                  StateDirectory = "eclssd";
                  # Misc hardening --- eclssd shouldn't need any filesystem
                  # access other than `/dev/i2c-*`.
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                };
              };
          }
          (mkIf cfg.openPorts {
            networking.firewall.allowedTCPPorts = [ cfg.server.port ];
          })
          (mkIf (!cfg.logging.colors) {
            systemd.services.${name}.environment = {
              NOCOLOR = "true";
            };
          })
          (mkIf (!cfg.logging.timestamps) {
            systemd.services.${name}.environment = {
              ECLSS_LOG_NO_TIMESTAMPS = "true";
            };
          })
          (mkIf cfg.readoutd.${ssd1680}.enable (
            let readoutdName = "eclss-readoutd"; in {

              # eclssd user/group. the service requires its own user in order to
              # add the "gpio" and "spi" groups.
              users = {
                users.${readoutdName} = {
                  inherit description;
                  isSystemUser = true;
                  group = readoutdName;
                  extraGroups = [ "gpio" "spi" ];
                };
                groups.${readoutdName} = { };
              };

              systemd.services."${readoutdName}-${ssd1680}" = {
                inherit description;
                wantedBy = [ "multi-user.target" ];
                after = [ "${name}.service" ];
                environment = {
                  ECLSS_LOG = cfg.logging.filter;
                  ECLSS_LOG_FORMAT = cfg.logging.format;
                };
                serviceConfig = {
                  User = readoutdName;
                  Group = readoutdName;
                  ExecStart = ''${eclssPkg}/bin/${readoutdName} \
                    localhost \
                    --port ${toString cfg.server.port} \
                    ${ssd1680}
                  '';
                  Restart = "on-failure";
                  RestartSec = "5s";
                  # Ensure that the "API VFS" (i.e. /dev/gpiomem) is mounted for
                  # the service.
                  MountAPIVFS = true;
                  # Ensure the system has access to real hardware devices in
                  # /dev
                  PrivateDevices = false;
                  # Ensure the service has access to the network so that it can
                  # bind its listener.
                  PrivateNetwork = false;
                  StateDirectory = "${readoutdName}-${ssd1680}";
                  # Misc hardening --- eclssd-readoutd shouldn't need any filesystem
                  # access other than `/dev/gpiomem` and `/dev/spidev`.
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                };
              };
            }
          ))
        ]);
      };
    };
}
