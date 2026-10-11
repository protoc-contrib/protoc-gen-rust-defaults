{
  description = "protoc-gen-rust-defaults - A protoc plugin that emits set_defaults() for buffa messages from protoc.contrib.defaults annotations";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nix-release-bin = {
      url = "github:nixos-contrib/nix-release-bin";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
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
      nix-release-bin,
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

        source = pkgs.rustPlatform.buildRustPackage {
          pname = manifest.name;
          inherit (manifest) version;
          src = pkgs.lib.cleanSource ./.;
          doCheck = false;

          cargoLock = {
            lockFile = ./Cargo.lock;
            # Git dependencies are fetched outside the lock's checksums. CI's
            # hash job repairs this on a Dependabot bump of the protoc-types
            # rev; bump it by hand with a manual one.
            outputHashes."protoc-types-0.1.0" = "sha256-GxQqPTfFZallZ+acrWVJCIFJgGQsgtzhcc1Q0xfYuAw=";
          };

          # Only the plugin, and the protoc-types crate it reads the options
          # through, whose generated code is committed: building needs no
          # protoc. The fixture exists to regenerate and type-check the test
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
      in
      {
        packages = {
          # The latest release binary, where it has one for the system: CI pins
          # them in the manifest once the release has published them. Until the
          # first release there is none, and every system builds from source.
          default = nix-release-bin.lib.mkReleaseBin {
            inherit pkgs;
            manifest = ./.github/config/nix-release-bin-manifest.json;
            pname = manifest.name;
            fallback = source;
          };
          inherit source;
        };

        devShells.default = pkgs.mkShell {
          inherit (manifest) name;
          # protoc for the tests: the fixture compiles the test schema.
          packages = [
            rust-toolchain
            pkgs.protobuf
          ];
        };
      }
    );
}
