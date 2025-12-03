{ lib
, rustPlatform
, pkg-config
, nix-eval-jobs
, git
}:
let
  runtime-deps = [
    nix-eval-jobs
    git
  ];
in
rustPlatform.buildRustPackage {
  name = "icicle";
  src = lib.cleanSource ./.;
  nativeBuildInputs = [
    pkg-config
  ];
  buildInputs = runtime-deps;
  passthru.runtime-deps = runtime-deps;
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = { };
  };
}
