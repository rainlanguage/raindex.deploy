{ pkgs, craneLib }:

let
  sourceRoot = toString ./.;
  source = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      let
        relative = pkgs.lib.removePrefix "${sourceRoot}/" (toString path);
      in
      pkgs.lib.cleanSourceFilter path type
      && relative != ".raindex"
      && !(pkgs.lib.hasPrefix ".raindex/" relative)
      && relative != "infra"
      && !(pkgs.lib.hasPrefix "infra/" relative);
  };
  commonArgs = {
    pname = "raindex-api";
    version = "0.0.0-alpha.0";
    src = source;
    strictDeps = true;
    cargoExtraArgs = "-p raindex_rest_api";
    nativeBuildInputs = [
      pkgs.pkg-config
    ]
    ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.darwin.DarwinTools ];
    buildInputs = [
      pkgs.libusb1
      pkgs.openssl
      pkgs.sqlite
    ]
    ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [ pkgs.apple-sdk_15 ];
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
{
  package = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      doCheck = true;
      cargoTestExtraArgs = "-p raindex_rest_api";
      meta = {
        description = "Raindex public market data API";
        homepage = "https://github.com/rainlanguage/rain.orderbook";
      };
    }
  );

  clippy = craneLib.cargoClippy (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoClippyExtraArgs = "-p raindex_rest_api --all-targets -- -D warnings";
    }
  );
}
