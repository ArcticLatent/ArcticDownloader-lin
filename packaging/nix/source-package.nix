{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  wrapGAppsHook3,
  makeWrapper,
  gtk3,
  webkitgtk_4_1,
  libayatana-appindicator,
  glib-networking,
  gst_all_1,
  dbus,
  git,
  curl,
  wget,
  python3,
  gcc,
  gnumake,
  cmake,
  ninja,
  coreutils,
  findutils,
  gnugrep,
  gnused,
  pciutils,
  procps,
  uv,
  util-linux,
  xdg-utils,
  arcticSupabaseUrl ? "",
  arcticSupabaseAnonKey ? "",
  arcticSupabasePublishableKey ? "",
}:

rustPlatform.buildRustPackage {
  pname = "arctic-comfyui-helper";
  version = "0.2.6";

  # Keep local release artifacts and Cargo build directories out of the Nix
  # source closure. This matters when the release script is run with `path:`
  # from a working tree that already contains dist/ and target/.
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../src
      ../../src-tauri/Cargo.toml
      ../../src-tauri/build.rs
      ../../src-tauri/capabilities
      ../../src-tauri/dist
      ../../src-tauri/icons
      ../../src-tauri/src
      ../../src-tauri/tauri.conf.json
      # Cargo resolves every workspace member before building the selected
      # app manifest, so the release-only signer must be present in the
      # filtered source closure even though it is not shipped in the package.
      ../../tools/manifest-signer
      ../../vendor
      ../../README.public.md
      ../../LICENSE
      ../../packaging/linux/io.github.ArcticHelper.desktop
    ];
  };
  cargoLock.lockFile = ../../Cargo.lock;
  cargoBuildFlags = [
    "--manifest-path"
    "src-tauri/Cargo.toml"
  ];

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook3
    makeWrapper
  ];

  buildInputs = [
    gtk3
    webkitgtk_4_1
    libayatana-appindicator
    glib-networking
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    dbus
  ];

  # CI runs the Rust test harness separately; avoid rebuilding the full Tauri
  # application a second time in the package derivation.
  doCheck = false;

  ARCTIC_SUPABASE_URL = arcticSupabaseUrl;
  ARCTIC_SUPABASE_ANON_KEY = arcticSupabaseAnonKey;
  ARCTIC_SUPABASE_PUBLISHABLE_KEY = arcticSupabasePublishableKey;

  installPhase = ''
    runHook preInstall

    install -Dm755 target/${stdenv.hostPlatform.rust.rustcTarget}/release/Arctic-ComfyUI-Helper \
      "$out/bin/arctic-comfyui-helper"
    install -Dm644 packaging/linux/io.github.ArcticHelper.desktop \
      "$out/share/applications/io.github.ArcticHelper.desktop"
    install -Dm644 src-tauri/dist/icon.svg \
      "$out/share/icons/hicolor/scalable/apps/io.github.ArcticHelper.svg"
    install -Dm644 README.public.md \
      "$out/share/doc/arctic-comfyui-helper/README.md"
    install -Dm644 LICENSE \
      "$out/share/licenses/arctic-comfyui-helper/LICENSE"

    runHook postInstall
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --prefix PATH : ${
        lib.makeBinPath [
          git
          curl
          wget
          python3
          gcc
          gnumake
          cmake
          ninja
          coreutils
          findutils
          gnugrep
          gnused
          pciutils
          procps
          uv
          util-linux
          xdg-utils
        ]
      }
      --set ARCTIC_PACKAGE_MANAGER nix
      --set ARCTIC_SKIP_AUTO_UPDATE 1
    )
  '';

  meta = {
    description = "ComfyUI installer and model manager";
    homepage = "https://github.com/ArcticLatent/Arctic-Helper";
    license = lib.licenses.asl20;
    mainProgram = "arctic-comfyui-helper";
    platforms = [ "x86_64-linux" ];
  };
}
