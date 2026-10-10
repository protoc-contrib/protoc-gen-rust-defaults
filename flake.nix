{
  description = "protoc-gen-rust-defaults - A protoc plugin that emits set_defaults() for buffa messages from protoc_contrib.defaults annotations";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        rust-toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = manifest.name;
          inherit (manifest) version;
          src = pkgs.lib.cleanSource ./.;
          doCheck = false;

          cargoLock.lockFile = ./Cargo.lock;

          # Only the plugin, and the protoc_contrib/defaults crate it reads the
          # options through, whose generated code is committed: building needs
          # no protoc. The fixture exists to regenerate and type-check the test
          # schema and has no place in the binary being shipped.
          cargoBuildFlags = [
            "--package"
            "protoc-gen-rust-defaults"
          ];

          meta = with pkgs.lib; {
            inherit (manifest) description homepage;
            license = licenses.mit;
            mainProgram = manifest.name;
          };
        };

        devShells.default = pkgs.mkShell {
          inherit (manifest) name;
          # protoc for the tests: the fixture compiles the test schema, and
          # protoc_contrib/defaults checks its committed code against a fresh
          # generation.
          packages = [
            rust-toolchain
            pkgs.protobuf
          ];
        };
      }
    );
}
