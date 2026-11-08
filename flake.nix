{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/7dcdd87373d72e7a2173e4db02e20d3d7f495cfd";
  };
  outputs =
    { nixpkgs, ... }:
    let
      eachSupportedSystem = nixpkgs.lib.genAttrs nixpkgs.lib.systems.flakeExposed;
      eachPkgs = f: eachSupportedSystem (system: f nixpkgs.legacyPackages.${system});
    in
    builtins.mapAttrs (_: eachPkgs) {
      formatter = pkgs: pkgs.nixfmt;

      devShells = pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [
            pkgs.libxkbcommon
          ];
          packages = [
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
          ];
        };
      };

      packages =
        {
          lib,
          rustPlatform,
          pkg-config,
          libxkbcommon,
          ...
        }:
        rec {
          default = rustPlatform.buildRustPackage {
            pname = "wlpp";
            version = "0.1.0";
            cargoHash = "sha256-K0OefqBlRm+KYuZCxfi1NFF6or950Zbi4uLDEy00tuo=";

            nativeBuildInputs = [
              pkg-config
            ];
            buildInputs = [
              libxkbcommon
            ];

            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.intersection (lib.fileset.fromSource (lib.sources.cleanSource ./.)) (
                lib.fileset.unions [
                  ./src
                  ./Cargo.toml
                  ./Cargo.lock
                ]
              );
            };
          };
          wlpp = default;
        };
    };
}
