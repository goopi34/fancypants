{
  description = "Fancypants — BLE rangefinder to Buttplug.io middleware and GUI application";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};

          commonBuildInputs = with pkgs; [
            dbus
            udev
            openssl
          ];

          guiBuildInputs = commonBuildInputs ++ (with pkgs; [
            glib
            gtk3
            webkitgtk_4_1
            libsoup_3
            cairo
            pango
            gdk-pixbuf
            atk
            librsvg
            libayatana-appindicator
          ]);

          fancypants-cli = pkgs.rustPlatform.buildRustPackage {
            pname = "fancypants";
            version = "0.4.2";
            src = ./.;
            cargoRoot = "middleware";
            buildAndTestSubdir = "middleware";
            cargoLock = {
              lockFile = ./middleware/Cargo.lock;
            };
            nativeBuildInputs = with pkgs; [ pkg-config ];
            buildInputs = commonBuildInputs;
            env.FANCYPANTS_VERSION = "0.4.2";
            doCheck = false;
          };

          fancypants-app = pkgs.rustPlatform.buildRustPackage {
            pname = "fancypants-app";
            version = "0.4.2";
            src = ./.;
            cargoRoot = "app/src-tauri";
            buildAndTestSubdir = "app/src-tauri";
            cargoLock = {
              lockFile = ./app/src-tauri/Cargo.lock;
            };
            nativeBuildInputs = with pkgs; [
              pkg-config
              wrapGAppsHook3
            ];
            buildInputs = guiBuildInputs;
            env.FANCYPANTS_VERSION = "0.4.2";
            preFixup = ''
              gappsWrapperArgs+=(
                --set WEBKIT_DISABLE_DMABUF_RENDERER 1
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath [ pkgs.libayatana-appindicator ]}"
              )
            '';
            doCheck = false;
          };
        in
        {
          default = fancypants-app;
          app = fancypants-app;
          cli = fancypants-cli;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.app}/bin/fancypants-app";
        };
        app = {
          type = "app";
          program = "${self.packages.${system}.app}/bin/fancypants-app";
        };
        cli = {
          type = "app";
          program = "${self.packages.${system}.cli}/bin/fancypants";
        };
      });

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              pkg-config
              rustc
              cargo
              clippy
              rustfmt
              cargo-tauri
              wrapGAppsHook3
              clang-tools
              openscad
              zizmor
            ];

            buildInputs = with pkgs; [
              dbus
              udev
              openssl
              glib
              gtk3
              webkitgtk_4_1
              libsoup_3
              cairo
              pango
              gdk-pixbuf
              atk
              librsvg
              libayatana-appindicator
              glib-networking
            ];

            shellHook = ''
              export WEBKIT_DISABLE_DMABUF_RENDERER=1
              export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
            '';
          };
        }
      );
    };
}
